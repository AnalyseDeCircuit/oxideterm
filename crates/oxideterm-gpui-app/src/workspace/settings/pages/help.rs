const HELP_WEBSITE_URL: &str = "https://oxideterm.app";
const HELP_DOCUMENTATION_URL: &str = "https://oxideterm.app/docs";
const HELP_GITHUB_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm";
const HELP_ISSUES_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm/issues";
// Keep the in-app legal link aligned with the repository-level multilingual notice.
const HELP_LEGAL_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm/blob/main/LEGAL.md";
const HELP_LEGAL_MARKDOWN: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../LEGAL.md"));

const HELP_TECH_BADGES: [(&str, u32); 6] = [
    ("Rust", 0xf97316),
    ("GPUI", 0x38bdf8),
    ("Tokio", 0x3b82f6),
    ("SSH", 0xeab308),
    ("redb", 0x22c55e),
    ("Portable Runtime", 0xa855f7),
];

const HELP_UPDATE_CHANNEL_SELECT_WIDTH: f32 = 140.0;
const HELP_UPDATE_PROXY_MODE_SELECT_WIDTH: f32 = 148.0;
const HELP_UPDATE_PROXY_PROTOCOL_SELECT_WIDTH: f32 = 120.0;
const HELP_UPDATE_PROXY_PORT_INPUT_WIDTH: f32 = 104.0;
const HELP_UPDATE_PROXY_TEXT_INPUT_WIDTH: f32 = 260.0;
const HELP_PREVIEW_NOTICE_ALPHA: f32 = 0.10;
const HELP_PREVIEW_NOTICE_BORDER_ALPHA: f32 = 0.30;
const HELP_UPDATE_FOOTER_BORDER_ALPHA: f32 = 0.50;
const HELP_PORTABLE_NOTICE_BG_ALPHA: f32 = 0.70;
const HELP_PORTABLE_NOTICE_BORDER_ALPHA: f32 = 0.60;
const HELP_RELEASE_NOTES_VIEWPORT_HEIGHT: f32 = 280.0;
const HELP_LEGAL_NOTICE_WIDTH: f32 = 760.0;
const HELP_LEGAL_NOTICE_HEIGHT: f32 = 720.0;

impl WorkspaceApp {
    fn settings_help_section(&mut self, section_index: usize, cx: &mut Context<Self>) -> AnyElement {
        match section_index {
            0 => self.help_version_card(cx),
            1 => self.help_diagnostics_card(cx),
            2 => self.help_tech_stack_card(),
            3 => self.help_resources_card(cx),
            4 => self.help_safety_card(),
            5 => self.help_legal_card(cx),
            _ => div().into_any_element(),
        }
    }

    fn help_version_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let is_portable = self.resolved_help_portable_mode();
        let channel_label = update_channel_label(
            self.settings_store.settings().general.update_channel,
            &self.i18n,
        );
        let update_channel = self.settings_store.settings().general.update_channel;
        let mut version_rows = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.help_key_value_row(
                "settings_view.help.app_name",
                "OxideTerm".to_string(),
                false,
                cx,
            ))
            .child(self.help_key_value_row(
                "settings_view.help.version",
                env!("CARGO_PKG_VERSION").to_string(),
                true,
                cx,
            ))
            .child(self.help_portable_or_channel_row(is_portable, channel_label, cx));

        if !is_portable {
            version_rows = version_rows.child(self.help_update_proxy_rows(cx));
        }

        if !is_portable && update_channel == UpdateChannel::GpuiPreview {
            version_rows = version_rows.child(self.help_gpui_preview_notice());
        }

        // Tauri HelpAboutSection keeps the version rows and update controls inside one
        // card, with only the update block separated by `border-t pt-4`.
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .child(
                div()
                    .mb(px(16.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(self.i18n.t("settings_view.help.version_info").to_uppercase()),
            )
            .child(version_rows)
            .child(self.help_update_footer(is_portable, cx));

        self.settings_card_surface(card, self.tokens.ui.bg_card)
            .into_any_element()
    }

    fn help_diagnostics_card(&self, cx: &mut Context<Self>) -> AnyElement {
        // MemoryDiagnosticsPanel and the keyboard-shortcut reference are Tauri-only Help blocks.
        // GPUI keeps diagnostics limited to log discovery so the page does not start samplers or
        // duplicate the dedicated keybindings settings surface.
        self.plain_settings_card(vec![
            self.card_title("settings_view.help.diagnostics"),
            self.help_action_row(
                "settings_view.help.open_logs",
                "settings_view.help.open_logs_hint",
                self.i18n.t("settings_view.help.open"),
                LucideIcon::FolderOpen,
                |this, _event, _window, cx| this.open_help_log_directory(cx),
                cx,
            ),
        ])
    }

    fn help_tech_stack_card(&self) -> AnyElement {
        let mut badges = div().flex().flex_row().flex_wrap().gap(px(8.0));
        for (label, color) in HELP_TECH_BADGES {
            badges = badges.child(
                div()
                    .rounded_full()
                    .bg(rgba((color << 8) | 0x26))
                    .px(px(10.0))
                    .py(px(4.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(color))
                    .child(label),
            );
        }

        self.plain_settings_card(vec![
            self.card_title("settings_view.help.tech_stack"),
            badges.into_any_element(),
        ])
    }

    fn help_resources_card(&self, cx: &mut Context<Self>) -> AnyElement {
        self.plain_settings_card(vec![
            self.card_title("settings_view.help.resources"),
            self.help_resource_link(
                "settings_view.help.website",
                HELP_WEBSITE_URL,
                LucideIcon::ExternalLink,
                cx,
            ),
            self.help_resource_link(
                "settings_view.help.documentation",
                HELP_DOCUMENTATION_URL,
                LucideIcon::BookOpen,
                cx,
            ),
            self.help_resource_link(
                "settings_view.help.github",
                HELP_GITHUB_URL,
                LucideIcon::GitFork,
                cx,
            ),
            self.help_resource_link(
                "settings_view.help.issues",
                HELP_ISSUES_URL,
                LucideIcon::HelpCircle,
                cx,
            ),
            self.help_resource_link(
                "settings_view.help.disclaimer",
                HELP_LEGAL_URL,
                LucideIcon::Shield,
                cx,
            ),
        ])
    }

    fn help_safety_card(&self) -> AnyElement {
        // Keep product guardrails visible without turning them into a blocking legal agreement.
        let safety_items = [
            "settings_view.help.safety_authorized",
            "settings_view.help.safety_connections",
            "settings_view.help.safety_prohibited",
            "settings_view.help.safety_privacy",
            "settings_view.help.safety_secrets",
            "settings_view.help.safety_ai",
        ];
        let mut safety_rows = div().flex().flex_col().gap(px(10.0));
        for key in safety_items {
            safety_rows = safety_rows.child(self.help_safety_row(key));
        }

        self.plain_settings_card(vec![
            self.card_title("settings_view.help.safety_title"),
            safety_rows.into_any_element(),
        ])
    }

    fn help_safety_row(&self, key: &str) -> AnyElement {
        div()
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .mt(px(7.0))
                    .size(px(5.0))
                    .rounded_full()
                    .bg(rgb(self.tokens.ui.accent)),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(key)),
            )
            .into_any_element()
    }

    fn help_legal_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let copyright = self.i18n_with(
            "settings_view.help.copyright",
            &[
                ("year", chrono::Local::now().format("%Y").to_string()),
                ("author", "AnalyseDeCircuit".to_string()),
            ],
        );

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.render_selectable_text_scoped(
                "settings-help-legal",
                "copyright",
                copyright,
                self.tokens.ui.text_muted,
                cx,
            ))
            .child(self.render_selectable_text_scoped(
                "settings-help-legal",
                "license",
                self.i18n.t("settings_view.help.license"),
                self.tokens.ui.text_muted,
                cx,
            ))
            .into_any_element()
    }

    fn help_portable_or_channel_row(
        &self,
        is_portable: bool,
        channel_label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if is_portable {
            return self.setting_row(
                "settings_view.help.portable_mode",
                "settings_view.help.portable_mode_hint",
                self.help_pill_badge(
                    self.i18n.t("settings_view.help.updates_manual_only"),
                    self.tokens.ui.text,
                ),
                cx,
            );
        }

        self.setting_row(
            "settings_view.help.update_channel",
            "settings_view.help.update_channel_hint",
            self.settings_select_control(
                SettingsSelect::UpdateChannel,
                channel_label,
                false,
                Some(HELP_UPDATE_CHANNEL_SELECT_WIDTH),
                cx,
            ),
            cx,
        )
    }

    fn help_update_proxy_rows(&self, cx: &mut Context<Self>) -> AnyElement {
        let proxy = &self.settings_store.settings().general.update_proxy;
        let mut rows = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.help_update_proxy_mode_row(cx));

        if proxy.mode == UpdateProxyMode::Custom {
            rows = rows
                .child(self.setting_row(
                    "settings_view.help.update_proxy_protocol",
                    "settings_view.help.update_proxy_protocol_hint",
                    self.settings_select_control(
                        SettingsSelect::UpdateProxyProtocol,
                        update_proxy_protocol_label(proxy.protocol, &self.i18n),
                        false,
                        Some(HELP_UPDATE_PROXY_PROTOCOL_SELECT_WIDTH),
                        cx,
                    ),
                    cx,
                ))
                .child(self.setting_row(
                    "settings_view.help.update_proxy_host",
                    "settings_view.help.update_proxy_host_hint",
                    self.settings_text_input_control(
                        SettingsInput::UpdateProxyHost,
                        self.current_settings_input_value(SettingsInput::UpdateProxyHost),
                        "127.0.0.1".to_string(),
                        HELP_UPDATE_PROXY_TEXT_INPUT_WIDTH,
                        cx,
                    ),
                    cx,
                ))
                .child(self.setting_row(
                    "settings_view.help.update_proxy_port",
                    "settings_view.help.update_proxy_port_hint",
                    self.settings_text_input_control(
                        SettingsInput::UpdateProxyPort,
                        self.current_settings_input_value(SettingsInput::UpdateProxyPort),
                        "7890".to_string(),
                        HELP_UPDATE_PROXY_PORT_INPUT_WIDTH,
                        cx,
                    ),
                    cx,
                ))
                .child(self.setting_row(
                    "settings_view.help.update_proxy_no_proxy",
                    "settings_view.help.update_proxy_no_proxy_hint",
                    self.settings_text_input_control(
                        SettingsInput::UpdateProxyNoProxy,
                        self.current_settings_input_value(SettingsInput::UpdateProxyNoProxy),
                        "localhost,127.0.0.1".to_string(),
                        HELP_UPDATE_PROXY_TEXT_INPUT_WIDTH,
                        cx,
                    ),
                    cx,
                ));
        }

        rows.into_any_element()
    }

    fn help_update_proxy_mode_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let proxy = &self.settings_store.settings().general.update_proxy;
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
                    .min_w(px(0.0))
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
                                "settings_view.help.update_proxy",
                                self.i18n.t("settings_view.help.update_proxy"),
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
                                "settings_view.help.update_proxy_hint",
                                self.i18n.t("settings_view.help.update_proxy_hint"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.render_selectable_text_scoped(
                                "settings-row-hint",
                                "settings_view.help.update_proxy_legal_hint",
                                self.i18n.t("settings_view.help.update_proxy_legal_hint"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            )
            .child(self.settings_select_control(
                SettingsSelect::UpdateProxyMode,
                update_proxy_mode_label(proxy.mode, &self.i18n),
                false,
                Some(HELP_UPDATE_PROXY_MODE_SELECT_WIDTH),
                cx,
            ))
            .into_any_element()
    }

    fn help_update_footer(&self, is_portable: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .mt(px(16.0))
            .pt(px(16.0))
            .border_t_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | alpha_byte(HELP_UPDATE_FOOTER_BORDER_ALPHA),
            ))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(if is_portable {
                self.help_portable_update_notice()
            } else {
                self.help_update_status_area(cx)
            })
            .into_any_element()
    }

    fn help_portable_update_notice(&self) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | alpha_byte(HELP_PORTABLE_NOTICE_BORDER_ALPHA),
            ))
            .bg(rgba(
                (self.tokens.ui.bg_elevated << 8) | alpha_byte(HELP_PORTABLE_NOTICE_BG_ALPHA),
            ))
            .p(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Shield,
                        16.0,
                        rgb(self.tokens.ui.warning),
                    ))
                    .child(self.i18n.t("settings_view.help.updates_manual_only")),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.help.updates_manual_only_hint")),
            )
            .into_any_element()
    }

    fn help_update_status_area(&self, cx: &mut Context<Self>) -> AnyElement {
        let button_icon = if matches!(self.native_update_state, NativeUpdateUiState::Checking) {
            LucideIcon::LoaderCircle
        } else {
            LucideIcon::RefreshCw
        };
        let disabled = matches!(
            self.native_update_state,
            NativeUpdateUiState::Checking
                | NativeUpdateUiState::Downloading(_)
                | NativeUpdateUiState::Verifying(_)
                | NativeUpdateUiState::Installing(_)
        );

        let mut area = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(self.help_outline_button_with_disabled(
                        self.i18n.t("settings_view.help.check_update"),
                        button_icon,
                        disabled,
                        |this, _event, _window, cx| {
                            this.check_native_update(cx);
                        },
                        cx,
                    ))
                    .children(self.help_update_status_inline()),
            );

        if let Some(detail) = self.help_update_detail(cx) {
            area = area.child(detail);
        }

        area.into_any_element()
    }

    fn help_update_status_inline(&self) -> Option<AnyElement> {
        let (label, icon, color) = match &self.native_update_state {
            NativeUpdateUiState::Checking => (
                self.i18n.t("settings_view.help.checking"),
                None,
                self.tokens.ui.text_muted,
            ),
            NativeUpdateUiState::UpToDate => (
                self.i18n.t("settings_view.help.up_to_date"),
                Some(LucideIcon::CheckCircle),
                self.tokens.ui.success,
            ),
            NativeUpdateUiState::Verifying(_) => (
                self.i18n.t("settings_view.help.verifying"),
                None,
                self.tokens.ui.text_muted,
            ),
            NativeUpdateUiState::Installing(plan) => (
                plan.as_ref()
                    .map(|plan| plan.summary.clone())
                    .unwrap_or_else(|| self.i18n.t("settings_view.help.installing")),
                None,
                self.tokens.ui.text_muted,
            ),
            NativeUpdateUiState::Downloaded(_) => (
                self.i18n.t("settings_view.help.update_downloaded"),
                Some(LucideIcon::CheckCircle),
                self.tokens.ui.success,
            ),
            NativeUpdateUiState::InstallFinished(outcome) => {
                let label_key = match outcome.status {
                    oxideterm_update::NativeInstallStatus::ManualActionRequired => {
                        "settings_view.help.update_downloaded"
                    }
                    oxideterm_update::NativeInstallStatus::InstallerLaunched => {
                        "settings_view.help.installer_launched"
                    }
                    oxideterm_update::NativeInstallStatus::ReplacementScheduled => {
                        "settings_view.help.replacement_scheduled"
                    }
                };
                (self.i18n.t(label_key), Some(LucideIcon::CheckCircle), self.tokens.ui.success)
            }
            NativeUpdateUiState::Error(error) => (
                if error.is_empty() {
                    self.i18n.t("settings_view.help.update_error")
                } else {
                    error.clone()
                },
                Some(LucideIcon::AlertCircle),
                self.tokens.ui.error,
            ),
            NativeUpdateUiState::Idle
            | NativeUpdateUiState::Available(_)
            | NativeUpdateUiState::Downloading(_) => return None,
        };

        let mut row = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(color));
        if let Some(icon) = icon {
            row = row.child(Self::render_lucide_icon(icon, 14.0, rgb(color)));
        }
        Some(row.child(label).into_any_element())
    }

    fn help_update_detail(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        match &self.native_update_state {
            NativeUpdateUiState::Available(package) => Some(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.help.update_available"))
                            .child(
                                div()
                                    .text_color(rgb(self.tokens.ui.accent))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(format!("v{}", package.version)),
                            ),
                    )
                    .child(self.help_release_notes(package.body.as_deref(), cx))
                    .child(
                        div().flex().justify_end().child(self.help_outline_button(
                            self.i18n.t("settings_view.help.download_update"),
                            LucideIcon::Download,
                            |this, _event, _window, cx| {
                                this.download_native_update(cx);
                            },
                            cx,
                        )),
                    )
                    .into_any_element(),
            ),
            NativeUpdateUiState::Downloading(status) | NativeUpdateUiState::Verifying(status) => {
                Some(self.help_transfer_progress(status.as_ref(), cx))
            }
            NativeUpdateUiState::Downloaded(_) => Some(
                div()
                    .flex()
                    .justify_end()
                    .child(self.help_outline_button(
                        self.i18n.t("settings_view.help.install_update"),
                        LucideIcon::Download,
                        |this, _event, _window, cx| {
                            this.install_native_update(cx);
                        },
                        cx,
                    ))
                    .into_any_element(),
            ),
            _ => None,
        }
    }

    fn help_gpui_preview_notice(&self) -> AnyElement {
        // Tauri renders `border-amber-500/30 bg-amber-500/10 p-3`; keep the
        // channel warning visually coupled to that source state.
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.warning << 8) | alpha_byte(HELP_PREVIEW_NOTICE_BORDER_ALPHA),
            ))
            .bg(rgba(
                (self.tokens.ui.warning << 8) | alpha_byte(HELP_PREVIEW_NOTICE_ALPHA),
            ))
            .p(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.warning))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Shield,
                        16.0,
                        rgb(self.tokens.ui.warning),
                    ))
                    .child(self.i18n.t("settings_view.help.gpui_preview_title")),
            )
            .child(
                div()
                    .mt(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(24.0))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("settings_view.help.gpui_preview_hint")),
            )
            .into_any_element()
    }

    fn resolved_help_portable_mode(&self) -> bool {
        self.portable_status_snapshot
            .as_ref()
            .map(|status| status.is_portable)
            .unwrap_or_else(|| oxideterm_portable_runtime::is_portable_mode().unwrap_or(false))
    }

    fn help_key_value_row(
        &self,
        label_key: &str,
        value: String,
        mono: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut value_el = div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.render_selectable_text_scoped(
                "settings-help-value",
                label_key,
                value.clone(),
                self.tokens.ui.text,
                cx,
            ));
        if mono {
            value_el = value_el.font_family(settings_mono_font_family(self.settings_store.settings()));
        }

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(value_el)
            .into_any_element()
    }

    fn help_action_row(
        &self,
        label_key: &str,
        hint_key: &str,
        button_label: String,
        icon: LucideIcon,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            self.help_outline_button(button_label, icon, listener, cx),
            cx,
        )
    }

    fn help_outline_button(
        &self,
        label: String,
        icon: LucideIcon,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 14.0, rgb(self.tokens.ui.text)).into_any_element()),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, event, window, cx| {
                listener(this, event, window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn help_outline_button_with_disabled(
        &self,
        label: String,
        icon: LucideIcon,
        disabled: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(icon, 14.0, rgb(self.tokens.ui.text)).into_any_element()),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                icon_position: ToolbarButtonIconPosition::Leading,
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, event, window, cx| {
                if !disabled {
                    listener(this, event, window, cx);
                }
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn help_pill_badge(&self, label: String, color: u32) -> AnyElement {
        div()
            .rounded_full()
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .px(px(12.0))
            .py(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(color))
            .child(label)
            .into_any_element()
    }

    fn help_release_notes(
        &self,
        release_body: Option<&str>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(release_body) = release_body.filter(|body| !body.trim().is_empty()) else {
            return div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.help.no_changelog"))
                .into_any_element();
        };

        let mut options = self.localized_markdown_options();
        options.base_font_size = self.tokens.metrics.ui_text_sm;
        options.block_gap = 8.0;
        let code_actions = self.markdown_mermaid_actions(cx);

        div()
            .h(px(HELP_RELEASE_NOTES_VIEWPORT_HEIGHT))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.border << 8) | alpha_byte(HELP_UPDATE_FOOTER_BORDER_ALPHA),
            ))
            .bg(rgba((self.tokens.ui.bg << 8) | alpha_byte(HELP_UPDATE_FOOTER_BORDER_ALPHA)))
            .p(px(12.0))
            .text_color(rgb(self.tokens.ui.text))
            .child(
                // Release notes can be long; use the shared virtual markdown
                // renderer so headings, lists, links, and code blocks match
                // other native markdown surfaces without rendering every block.
                markdown_virtual_with_code_actions(
                    cx.entity(),
                    "settings-help-release-notes-markdown",
                    &self.tokens,
                    release_body,
                    &options,
                    &self.native_update_release_notes_scroll,
                    &code_actions,
                ),
            )
            .into_any_element()
    }

    fn help_transfer_progress(
        &self,
        status: Option<&oxideterm_update::ResumableUpdateStatus>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(
                        status
                            .map(native_update_progress_hint)
                            .unwrap_or_else(|| self.i18n.t("settings_view.help.downloading")),
                    ),
            )
            .child(self.help_outline_button(
                self.i18n.t("settings_view.help.cancel"),
                LucideIcon::X,
                |this, _event, _window, cx| {
                    this.cancel_native_update(cx);
                },
                cx,
            ))
            .into_any_element()
    }

    fn help_resource_link(
        &self,
        label_key: &str,
        url: &'static str,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.md))
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(self.tokens.ui.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if url == HELP_LEGAL_URL {
                        this.open_help_legal_notice(cx);
                    } else {
                        this.open_help_url(url, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(Self::render_lucide_icon(
                        icon,
                        18.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    ),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ExternalLink,
                14.0,
                rgb(self.tokens.ui.text_muted),
            ))
            .into_any_element()
    }

    pub(in crate::workspace) fn open_help_legal_notice(&mut self, cx: &mut Context<Self>) {
        self.settings_page.legal_notice_open = true;
        self.settings_legal_notice_scroll = MarkdownVirtualListScrollHandle::new();
        cx.notify();
    }

    fn close_help_legal_notice(&mut self, cx: &mut Context<Self>) {
        self.settings_page.legal_notice_open = false;
        cx.notify();
    }

    pub(in crate::workspace) fn handle_help_legal_notice_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.settings_page.legal_notice_open
            || event.keystroke.key.as_str() != "escape"
            || event.keystroke.modifiers.platform
        {
            return false;
        }
        self.close_help_legal_notice(cx);
        true
    }

    pub(in crate::workspace) fn render_help_legal_notice_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut options = self.localized_markdown_options();
        options.base_font_size = self.tokens.metrics.ui_text_sm;
        options.block_gap = 8.0;
        let code_actions = self.markdown_mermaid_actions(cx);

        dismissible_dialog_backdrop()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.close_help_legal_notice(cx);
                    cx.stop_propagation();
                }),
            )
            .child(overlay_content_boundary(
                dialog_content(&self.tokens)
                    .flex()
                    .flex_col()
                    .w(px(HELP_LEGAL_NOTICE_WIDTH))
                    .max_w(relative(0.92))
                    .h(px(HELP_LEGAL_NOTICE_HEIGHT))
                    .max_h(relative(0.90))
                    .child(
                        dialog_header(&self.tokens)
                            .child(dialog_title(
                                &self.tokens,
                                self.i18n.t("settings_view.help.disclaimer"),
                            ))
                            .child(dialog_description(
                                &self.tokens,
                                self.i18n.t("settings_view.help.legal_notice_description"),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h(px(0.0))
                            .p(px(16.0))
                            .bg(rgb(self.tokens.ui.bg))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(markdown_virtual_with_code_actions(
                                cx.entity(),
                                "settings-help-legal-notice-markdown",
                                &self.tokens,
                                HELP_LEGAL_MARKDOWN,
                                &options,
                                &self.settings_legal_notice_scroll,
                                &code_actions,
                            )),
                    )
                    .child(dialog_footer(&self.tokens).child(self.standard_footer_action_button(
                        self.i18n.t("settings_view.help.legal_notice_close"),
                        ButtonVariant::Secondary,
                        ConfirmDialogAction::Cancel,
                        false,
                        |this, _event, _window, cx| {
                            this.close_help_legal_notice(cx);
                        },
                        cx,
                    ))),
            ))
            .into_any_element()
    }

    fn open_help_url(&mut self, url: &'static str, cx: &mut Context<Self>) {
        if let Err(error) = open_external_url(url) {
            self.push_ai_settings_toast(error.to_string(), TerminalNoticeVariant::Error);
            cx.notify();
        }
    }

    fn open_help_log_directory(&mut self, cx: &mut Context<Self>) {
        let log_dir = self.help_log_directory();
        let opened = std::fs::create_dir_all(&log_dir)
            .and_then(|()| open_path_external(&log_dir))
            .map_err(|error| error.to_string());
        if let Err(error) = opened {
            self.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
            cx.notify();
        }
    }

    fn help_log_directory(&self) -> std::path::PathBuf {
        // Tauri stores logs under the app data directory. Native settings use
        // the same data root, so derive logs beside settings.json.
        self.settings_store
            .path()
            .parent()
            .map(|parent| parent.join("logs"))
            .unwrap_or_else(|| std::path::PathBuf::from("logs"))
    }

    fn language_label(&self, language: Language) -> String {
        match language {
            Language::De => "Deutsch",
            Language::En => "English",
            Language::EsEs => "Español (España)",
            Language::FrFr => "Français (France)",
            Language::It => "Italiano",
            Language::Ko => "한국어",
            Language::PtBr => "Português (Brasil)",
            Language::Vi => "Tiếng Việt",
            Language::Ja => "日本語",
            Language::ZhCn => "简体中文",
            Language::ZhTw => "繁體中文",
        }
        .to_string()
    }
}

fn native_update_progress_hint(status: &oxideterm_update::ResumableUpdateStatus) -> String {
    let downloaded = native_update_format_bytes(status.downloaded_bytes);
    match status.total_bytes {
        Some(total) if total > 0 => {
            let percent = (status.downloaded_bytes.saturating_mul(100) / total).min(100);
            format!(
                "{} / {} · {}% · attempt {}",
                downloaded,
                native_update_format_bytes(total),
                percent,
                status.attempt
            )
        }
        _ => format!("{} · attempt {}", downloaded, status.attempt),
    }
}

fn native_update_format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    for unit in ["KB", "MB", "GB"] {
        value /= 1024.0;
        if value < 1024.0 {
            return format!("{value:.1} {unit}");
        }
    }
    format!("{:.1} TB", value / 1024.0)
}
