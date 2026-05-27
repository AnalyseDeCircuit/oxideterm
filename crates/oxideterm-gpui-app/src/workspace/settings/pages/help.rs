const HELP_WEBSITE_URL: &str = "https://oxideterm.app";
const HELP_DOCUMENTATION_URL: &str = "https://oxideterm.app/docs";
const HELP_GITHUB_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm";
const HELP_RELEASES_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm/releases";
const HELP_ISSUES_URL: &str = "https://github.com/AnalyseDeCircuit/oxideterm/issues";
const HELP_DISCLAIMER_URL: &str =
    "https://github.com/AnalyseDeCircuit/oxideterm/blob/main/DISCLAIMER.md";

const HELP_TECH_BADGES: [(&str, u32); 6] = [
    ("Rust", 0xf97316),
    ("GPUI", 0x38bdf8),
    ("Tokio", 0x3b82f6),
    ("SSH", 0xeab308),
    ("redb", 0x22c55e),
    ("Portable Runtime", 0xa855f7),
];

const HELP_SHORTCUT_ROWS: [(&str, &str); 12] = [
    ("settings_view.help.shortcut_new_tab", "app.newTerminal"),
    ("settings_view.help.shortcut_shell_launcher", "app.shellLauncher"),
    ("settings_view.help.shortcut_close_tab", "app.closeTab"),
    ("settings_view.help.shortcut_close_other_tabs", "app.closeOtherTabs"),
    ("settings_view.help.shortcut_next_tab", "app.nextTab"),
    ("settings_view.help.shortcut_prev_tab", "app.prevTab"),
    ("settings_view.help.shortcut_new_connection", "app.newConnection"),
    ("settings_view.help.shortcut_command_palette", "app.commandPalette"),
    ("settings_view.help.shortcut_toggle_sidebar", "app.toggleSidebar"),
    ("settings_view.help.shortcut_settings", "app.settings"),
    ("settings_view.help.shortcut_find", "terminal.find"),
    ("settings_view.help.shortcut_ai_panel", "terminal.aiPanel"),
];

impl WorkspaceApp {
    fn settings_help_section(&mut self, section_index: usize, cx: &mut Context<Self>) -> AnyElement {
        match section_index {
            0 => self.help_version_card(cx),
            1 => self.help_diagnostics_card(cx),
            2 => self.help_tech_stack_card(),
            3 => self.help_shortcuts_card(cx),
            4 => self.help_resources_card(cx),
            5 => self.help_legal_card(cx),
            _ => div().into_any_element(),
        }
    }

    fn help_version_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let status = self.portable_status_snapshot.as_ref();
        let is_portable = status
            .map(|status| status.is_portable)
            .unwrap_or_else(|| oxideterm_portable_runtime::is_portable_mode().unwrap_or(false));
        let update_status = if is_portable {
            self.i18n.t("settings_view.help.updates_manual_only")
        } else {
            update_channel_label(self.settings_store.settings().general.update_channel, &self.i18n)
        };

        self.plain_settings_card(vec![
            self.card_title("settings_view.help.version_info"),
            self.help_key_value_row(
                "settings_view.help.app_name",
                "OxideTerm Native".to_string(),
                false,
                cx,
            ),
            self.help_key_value_row(
                "settings_view.help.version",
                env!("CARGO_PKG_VERSION").to_string(),
                true,
                cx,
            ),
            self.help_key_value_row(
                "settings_view.help.portable_mode",
                if is_portable {
                    self.i18n.t("common.enabled")
                } else {
                    self.i18n.t("common.disabled")
                },
                false,
                cx,
            ),
            self.card_separator(),
            self.setting_row(
                "settings_view.help.update_channel",
                "settings_view.help.update_channel_hint",
                self.help_update_channel_control(update_status, is_portable, cx),
                cx,
            ),
            self.help_update_notice(is_portable, cx),
        ])
    }

    fn help_diagnostics_card(&self, cx: &mut Context<Self>) -> AnyElement {
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
            self.card_separator(),
            // Tauri's Help page wires this to MemoryDiagnosticsPanel and a
            // frontend/backend sampling store. Native GPUI does not have that
            // diagnostics backend yet, so keep the row visible for parity but
            // mark it unavailable instead of showing a fake action.
            self.setting_row(
                "settings_view.help.memory_diagnostics_title",
                "settings_view.help.memory_diagnostics_hint",
                self.text_badge(self.i18n.t("common.disabled"), self.tokens.ui.text_muted),
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

    fn help_shortcuts_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let side = crate::keybindings::KeybindingSide::current();
        let overrides = &self.settings_store.settings().keybindings.overrides;
        let mut rows = Vec::with_capacity(HELP_SHORTCUT_ROWS.len() + 1);
        rows.push(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(self.tokens.ui.text))
                .child(Self::render_lucide_icon(
                    LucideIcon::Keyboard,
                    16.0,
                    rgb(self.tokens.ui.text),
                ))
                .child(self.i18n.t("settings_view.help.shortcuts").to_uppercase())
                .into_any_element(),
        );

        for (index, (label_key, action_id)) in HELP_SHORTCUT_ROWS.iter().enumerate() {
            if let Some(definition) = crate::keybindings::action_definition(action_id) {
                let combo = crate::keybindings::effective_combo(definition, overrides, side);
                rows.push(self.help_shortcut_row(
                    label_key,
                    &crate::keybindings::format_combo(&combo),
                    index + 1 < HELP_SHORTCUT_ROWS.len(),
                    cx,
                ));
            }
        }

        self.plain_settings_card(rows)
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
                HELP_DISCLAIMER_URL,
                LucideIcon::Shield,
                cx,
            ),
        ])
    }

    fn help_legal_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let copyright = self.i18n_with(
            "settings_view.help.copyright",
            &[
                ("year", chrono::Local::now().format("%Y").to_string()),
                ("author", "AnalyseDeCircuit".to_string()),
            ],
        );

        self.plain_settings_card(vec![
            self.card_title("settings_view.help.license"),
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
                .into_any_element(),
        ])
    }

    fn help_update_channel_control(
        &self,
        label: String,
        is_portable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if is_portable {
            return self.text_badge(label, self.tokens.ui.warning);
        }
        self.help_outline_button(
            label,
            LucideIcon::RefreshCw,
            |this, _event, _window, cx| {
                this.edit_settings(cycle_update_channel, cx);
            },
            cx,
        )
    }

    fn help_update_notice(&self, is_portable: bool, cx: &mut Context<Self>) -> AnyElement {
        let (title, hint, button_label, icon, disabled) = if is_portable {
            (
                self.i18n.t("settings_view.help.updates_manual_only"),
                self.i18n.t("settings_view.help.updates_manual_only_hint"),
                self.i18n.t("settings_view.help.check_update"),
                LucideIcon::RefreshCw,
                false,
            )
        } else {
            match &self.native_update_state {
                NativeUpdateUiState::Idle => (
                    self.i18n.t("settings_view.help.check_update"),
                    self.i18n.t("settings_view.help.native_update_hint"),
                    self.i18n.t("settings_view.help.check_update"),
                    LucideIcon::RefreshCw,
                    false,
                ),
                NativeUpdateUiState::Checking => (
                    self.i18n.t("settings_view.help.checking"),
                    self.i18n.t("settings_view.help.native_update_hint"),
                    self.i18n.t("settings_view.help.checking"),
                    LucideIcon::RefreshCw,
                    true,
                ),
                NativeUpdateUiState::UpToDate => (
                    self.i18n.t("settings_view.help.up_to_date"),
                    self.i18n.t("settings_view.help.native_update_hint"),
                    self.i18n.t("settings_view.help.check_update"),
                    LucideIcon::RefreshCw,
                    false,
                ),
                NativeUpdateUiState::Available(package) => (
                    format!(
                        "{} v{}",
                        self.i18n.t("settings_view.help.update_available"),
                        package.version
                    ),
                    package
                        .body
                        .clone()
                        .unwrap_or_else(|| self.i18n.t("settings_view.help.no_changelog")),
                    self.i18n.t("settings_view.help.download_update"),
                    LucideIcon::Download,
                    false,
                ),
                NativeUpdateUiState::Downloading(status) => (
                    self.i18n.t("settings_view.help.downloading"),
                    status
                        .as_ref()
                        .map(native_update_progress_hint)
                        .unwrap_or_else(|| self.i18n.t("settings_view.help.native_update_hint")),
                    self.i18n.t("settings_view.help.cancel"),
                    LucideIcon::X,
                    false,
                ),
                NativeUpdateUiState::Verifying(status) => (
                    self.i18n.t("settings_view.help.verifying"),
                    status
                        .as_ref()
                        .map(native_update_progress_hint)
                        .unwrap_or_else(|| self.i18n.t("settings_view.help.native_update_hint")),
                    self.i18n.t("settings_view.help.cancel"),
                    LucideIcon::X,
                    false,
                ),
                NativeUpdateUiState::Downloaded(download) => (
                    self.i18n.t("settings_view.help.update_downloaded"),
                    self.i18n_with(
                        "settings_view.help.update_downloaded_hint",
                        &[("path", download.path.display().to_string())],
                    ),
                    self.i18n.t("settings_view.help.install_update"),
                    LucideIcon::Download,
                    false,
                ),
                NativeUpdateUiState::Installing(plan) => (
                    self.i18n.t("settings_view.help.installing"),
                    plan.as_ref().map(|plan| plan.summary.clone()).unwrap_or_else(|| {
                        self.i18n.t("settings_view.help.native_update_hint")
                    }),
                    self.i18n.t("settings_view.help.installing"),
                    LucideIcon::Download,
                    true,
                ),
                NativeUpdateUiState::InstallFinished(outcome) => {
                    let (title_key, hint_key) = match outcome.status {
                        oxideterm_update::NativeInstallStatus::ManualActionRequired => (
                            "settings_view.help.update_downloaded",
                            "settings_view.help.portable_update_manual_hint",
                        ),
                        oxideterm_update::NativeInstallStatus::InstallerLaunched => (
                            "settings_view.help.installer_launched",
                            "settings_view.help.installer_launched_hint",
                        ),
                        oxideterm_update::NativeInstallStatus::ReplacementScheduled => (
                            "settings_view.help.replacement_scheduled",
                            "settings_view.help.replacement_scheduled_hint",
                        ),
                    };
                    (
                        self.i18n.t(title_key),
                        self.i18n.t(hint_key),
                        self.i18n.t("settings_view.help.check_update"),
                        LucideIcon::RefreshCw,
                        false,
                    )
                }
                NativeUpdateUiState::Error(error) => (
                    self.i18n.t("settings_view.help.update_error"),
                    error.clone(),
                    self.i18n.t("settings_view.help.retry"),
                    LucideIcon::RefreshCw,
                    false,
                ),
            }
        };

        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
            .bg(self.settings_panel_background(self.tokens.ui.bg))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
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
                        14.0,
                        rgb(if is_portable {
                            self.tokens.ui.warning
                        } else {
                            self.tokens.ui.accent
                        }),
                    ))
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(hint),
            )
            .child(self.help_outline_button(
                button_label,
                icon,
                move |this, _event, _window, cx| {
                    if disabled {
                        return;
                    }
                    if is_portable {
                        this.open_help_url(HELP_RELEASES_URL, cx);
                    } else if matches!(
                        this.native_update_state,
                        NativeUpdateUiState::Downloading(_) | NativeUpdateUiState::Verifying(_)
                    ) {
                        this.cancel_native_update(cx);
                    } else if matches!(
                        this.native_update_state,
                        NativeUpdateUiState::Available(_)
                    ) {
                        this.download_native_update(cx);
                    } else if matches!(this.native_update_state, NativeUpdateUiState::Downloaded(_))
                    {
                        this.install_native_update(cx);
                    } else {
                        this.check_native_update(cx);
                    }
                },
                cx,
            ))
            .into_any_element()
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

    fn help_shortcut_row(
        &self,
        label_key: &str,
        shortcut: &str,
        separator: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .py(px(6.0))
            .when(separator, |row| {
                row.border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x4d))
            })
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(self.keybinding_kbd_badge(shortcut, false, cx))
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
                    this.open_help_url(url, cx);
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
