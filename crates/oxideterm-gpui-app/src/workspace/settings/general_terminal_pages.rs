#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalCommandSpecsAction {
    Format,
    Example,
    Save,
}

const SETTINGS_TERMINAL_TEXTAREA_LINE_GAP: f32 = 2.0; // Existing GPUI visual gap between rendered textarea rows.

impl WorkspaceApp {
    fn settings_general_section(&self, section_index: usize, cx: &mut Context<Self>) -> AnyElement {
        let settings = self.settings_store.settings();
        match section_index {
            0 => self.settings_card(
                "settings_view.general.language",
                "settings_view.general.language_hint",
                vec![self.language_select_row(settings.general.language, cx)],
            ),
            1 => {
                let data_dir = self
                    .settings_store
                    .path()
                    .parent()
                    .unwrap_or_else(|| self.settings_store.path())
                    .display()
                    .to_string();
                self.plain_settings_card(vec![
                    self.card_title("settings_view.general.data_directory"),
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.i18n.t("settings_view.general.data_directory")),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.general.data_directory_hint")),
                        )
                        .into_any_element(),
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
                                .font_family(settings_mono_font_family(
                                    self.settings_store.settings(),
                                ))
                                .truncate()
                                .child(data_dir),
                        )
                        .child(
                            self.outline_button(
                                self.i18n.t("settings_view.general.change"),
                                ButtonSize::Sm,
                            ),
                        )
                        .into_any_element(),
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.warning))
                        .child(
                            self.i18n
                                .t("settings_view.general.data_directory_restart_notice"),
                        )
                        .into_any_element(),
                ])
            }
            2 => {
                let cli_path = std::env::var_os("HOME")
                    .map(|home| {
                        std::path::PathBuf::from(home)
                            .join(".local")
                            .join("bin")
                            .join("oxt")
                            .display()
                            .to_string()
                    })
                    .unwrap_or_else(|| "~/.local/bin/oxt".to_string());
                self.plain_settings_card(vec![
                    self.card_title("settings_view.general.cli_companion"),
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.i18n.t("settings_view.general.cli_tool")),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.i18n.t("settings_view.general.cli_tool_hint")),
                        )
                        .into_any_element(),
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
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Terminal,
                                            16.0,
                                            rgb(self.tokens.ui.text_muted),
                                        ))
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_family(settings_mono_font_family(
                                                    self.settings_store.settings(),
                                                ))
                                                .text_color(rgb(self.tokens.ui.text))
                                                .child("oxide"),
                                        )
                                        .child(self.text_badge(
                                            self.i18n.t("settings_view.general.cli_installed"),
                                            self.tokens.ui.success,
                                        )),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .min_w(px(0.0))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .font_family(settings_mono_font_family(
                                            self.settings_store.settings(),
                                        ))
                                        .text_color(rgb(self.tokens.ui.text_muted))
                                        .truncate()
                                        .child(cli_path),
                                ),
                        )
                        .child(
                            self.outline_button(
                                self.i18n.t("settings_view.general.cli_uninstall"),
                                ButtonSize::Sm,
                            ),
                        )
                        .into_any_element(),
                ])
            }
            _ => div().into_any_element(),
        }
    }

    fn settings_portable_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match section_index {
            0 => self.settings_card(
                "settings_view.general.portable_runtime",
                "settings_view.general.portable_runtime_disabled_hint",
                vec![
                    self.value_row(
                        "settings_view.general.portable_root_dir",
                        "settings_view.general.portable_runtime_hint",
                        self.i18n
                            .t("settings_view.general.portable_instance_lock_unavailable"),
                        cx,
                    ),
                    self.value_row(
                        "settings_view.general.portable_activation",
                        "settings_view.general.portable_runtime_hint",
                        self.i18n.t("settings_view.general.portable_activation_disabled"),
                        cx,
                    ),
                    self.value_row(
                        "settings_view.general.portable_config_path",
                        "settings_view.general.portable_runtime_hint",
                        self.i18n
                            .t("settings_view.general.portable_instance_lock_unavailable"),
                        cx,
                    ),
                    self.value_row(
                        "settings_view.general.portable_biometric",
                        "settings_view.general.portable_runtime_hint",
                        self.i18n
                            .t("settings_view.general.portable_biometric_unsupported"),
                        cx,
                    ),
                    self.value_row(
                        "settings_view.general.portable_change_password",
                        "settings_view.general.portable_runtime_hint",
                        self.i18n.t("common.disabled"),
                        cx,
                    ),
                    self.value_row(
                        "settings_view.general.cli_tool",
                        "settings_view.general.cli_tool_hint",
                        self.i18n.t("settings_view.general.cli_not_installed"),
                        cx,
                    ),
                    self.value_row(
                        "settings_view.general.cli_install",
                        "settings_view.general.cli_reinstall_hint",
                        self.i18n.t("settings_view.general.cli_not_bundled"),
                        cx,
                    ),
                ],
            ),
            _ => div().into_any_element(),
        }
    }

    fn settings_terminal_section(
        &self,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_store.settings();
        if section_index == 0 {
            return self.terminal_page_switcher(cx);
        }

        match (self.settings_page.terminal_page, section_index - 1) {
            (TerminalSettingsPage::Display, 0) => self.settings_card(
                "settings_view.terminal.font",
                "settings_view.terminal.font_family_hint",
                vec![
                    self.select_setting_row(
                        "settings_view.terminal.font_family",
                        "settings_view.terminal.font_family_hint",
                        SettingsSelect::TerminalFontFamily,
                        font_family_label(settings.terminal.font_family),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                    self.terminal_preview(settings),
                    self.card_separator(),
                    self.font_size_row(settings, cx),
                    self.card_separator(),
                    self.decimal_row(
                        "settings_view.terminal.line_height",
                        "settings_view.terminal.line_height_hint",
                        SettingsInput::TerminalLineHeight,
                        compact_decimal(settings.terminal.line_height),
                        cx,
                    ),
                    self.card_separator(),
                    self.select_setting_row(
                        "settings_view.terminal.encoding",
                        "settings_view.terminal.encoding_hint",
                        SettingsSelect::TerminalEncoding,
                        terminal_encoding_label(settings.terminal.terminal_encoding),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.show_fps_overlay",
                        "settings_view.terminal.show_fps_overlay_hint",
                        settings.terminal.show_fps_overlay,
                        set_show_fps_overlay,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Display, 1) => self.settings_card(
                "settings_view.terminal.cursor",
                "settings_view.terminal.cursor_style_hint",
                vec![
                    self.select_setting_row(
                        "settings_view.terminal.cursor_style",
                        "settings_view.terminal.cursor_style_hint",
                        SettingsSelect::TerminalCursorStyle,
                        cursor_style_label(settings.terminal.cursor_style, &self.i18n),
                        self.tokens.metrics.settings_select_narrow_width,
                        cx,
                    ),
                    self.card_separator(),
                    self.checkbox_row(
                        "settings_view.terminal.cursor_blink",
                        "settings_view.terminal.cursor_blink_hint",
                        settings.terminal.cursor_blink,
                        set_terminal_cursor_blink,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Input, 0) => self.terminal_input_settings_card(settings, cx),
            (TerminalSettingsPage::CommandBar, 0) => self.settings_card(
                "settings_view.terminal.command_bar",
                "settings_view.terminal.command_bar_hint",
                vec![
                    self.bool_row(
                        "settings_view.terminal.command_bar",
                        "settings_view.terminal.command_bar_hint",
                        settings.terminal.command_bar.enabled,
                        set_command_bar_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_bar_legacy_toolbar",
                        "settings_view.terminal.command_bar_legacy_toolbar_hint",
                        settings.terminal.command_bar.show_legacy_toolbar,
                        set_command_bar_legacy_toolbar,
                        cx,
                    ),
                    self.card_separator(),
                    self.focus_handoff_commands_row(settings, cx),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.quick_commands",
                        "settings_view.terminal.quick_commands_hint",
                        settings.terminal.command_bar.quick_commands_enabled,
                        set_quick_commands_enabled,
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
                    self.terminal_command_specs_editor_row(cx),
                ],
            ),
            (TerminalSettingsPage::History, 0) => self.settings_card(
                "settings_view.terminal.command_marks",
                "settings_view.terminal.command_marks_hint",
                vec![
                    self.bool_row(
                        "settings_view.terminal.command_marks",
                        "settings_view.terminal.command_marks_hint",
                        settings.terminal.command_marks.enabled,
                        set_command_marks_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.command_marks_hover_actions",
                        "settings_view.terminal.command_marks_hover_actions_hint",
                        settings.terminal.command_marks.show_hover_actions,
                        set_command_marks_hover_actions,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::History, 1) => self.settings_card(
                "settings_view.terminal.buffer",
                "settings_view.terminal.scrollback_hint",
                vec![
                    self.number_row(
                        "settings_view.terminal.scrollback",
                        "settings_view.terminal.scrollback_hint",
                        settings.terminal.scrollback,
                        500,
                        500,
                        20000,
                        set_terminal_scrollback,
                        cx,
                    ),
                    self.card_separator(),
                    self.number_row(
                        "settings_view.terminal.backend_buffer_lines",
                        "settings_view.terminal.backend_buffer_lines_hint",
                        settings.buffer.max_lines,
                        500,
                        5000,
                        12000,
                        set_buffer_max_lines,
                        cx,
                    ),
                ],
            ),
            (TerminalSettingsPage::Transfer, 0) => self.settings_card(
                "settings_view.terminal.in_band_transfer.title",
                "settings_view.terminal.in_band_transfer.runtime_note",
                vec![
                    self.bool_row(
                        "settings_view.terminal.in_band_transfer.enabled",
                        "settings_view.terminal.in_band_transfer.enabled_hint",
                        settings.terminal.in_band_transfer.enabled,
                        set_in_band_transfer_enabled,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.in_band_transfer.allow_directory",
                        "settings_view.terminal.in_band_transfer.allow_directory_hint",
                        settings.terminal.in_band_transfer.allow_directory,
                        set_in_band_transfer_allow_directory,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_chunk_bytes",
                        "settings_view.terminal.in_band_transfer.max_chunk_bytes_hint",
                        SettingsInput::InBandTransferMaxChunkBytes,
                        settings.terminal.in_band_transfer.max_chunk_bytes,
                        128.0,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_file_count",
                        "settings_view.terminal.in_band_transfer.max_file_count_hint",
                        SettingsInput::InBandTransferMaxFileCount,
                        settings.terminal.in_band_transfer.max_file_count,
                        128.0,
                        cx,
                    ),
                    self.card_separator(),
                    self.in_band_transfer_number_row(
                        "settings_view.terminal.in_band_transfer.max_total_bytes",
                        "settings_view.terminal.in_band_transfer.max_total_bytes_hint",
                        SettingsInput::InBandTransferMaxTotalBytes,
                        settings.terminal.in_band_transfer.max_total_bytes,
                        160.0,
                        cx,
                    ),
                    self.in_band_transfer_runtime_note(),
                ],
            ),
            (TerminalSettingsPage::Highlight, 0) => self.highlight_rules_card(settings, cx),
            _ => div().into_any_element(),
        }
    }

    fn focus_handoff_commands_row(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = SettingsInput::TerminalCommandBarFocusHandoff;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            settings
                .terminal
                .command_bar
                .focus_handoff_commands
                .join("\n")
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(96.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(2.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        if value.is_empty() {
            textarea = self.render_settings_multiline_textarea_lines(
                textarea,
                target,
                "vim\nnvim\nlazygit",
                true,
                line_height,
            );
        } else {
            textarea = self.render_settings_multiline_textarea_lines(
                textarea,
                target,
                &value,
                false,
                line_height,
            );
        }

        if let Some(marked) = self.marked_text_for_target(target) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }

        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("settings_view.terminal.command_bar_focus_handoff")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                self.i18n
                                    .t("settings_view.terminal.command_bar_focus_handoff_hint"),
                            ),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    fn terminal_command_specs_editor_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let input = SettingsInput::TerminalCommandSpecsJson;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.load_terminal_command_specs_editor_value()
        };
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let theme = self.tokens.ui;
        let line_height = input.textarea_line_height();
        let mut textarea = div()
            .w_full()
            .min_h(px(220.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if focused {
                rgba((theme.accent << 8) | 0x99)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg))
            .px(px(12.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .items_start()
            .gap(px(2.0))
            .cursor(CursorStyle::IBeam)
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_family(settings_mono_font_family(self.settings_store.settings()))
            .text_color(rgb(theme.text))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(|this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                }),
            );

        let display = if value.trim().is_empty() {
            super::terminal_command_bar::completion::terminal_command_specs_example_json()
        } else {
            value
        };
        textarea =
            self.render_settings_multiline_textarea_lines(textarea, target, &display, false, line_height);
        if let Some(marked) = self.marked_text_for_target(target) {
            textarea = textarea.child(
                div()
                    .underline()
                    .text_color(rgb(theme.text))
                    .child(marked.to_string()),
            );
        }
        let control = text_input_anchor_probe(target.anchor_id(), textarea, move |anchor, _window, cx| {
            let _ = workspace.update(cx, |this, cx| {
                this.update_text_input_anchor(anchor, cx);
            });
        });

        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
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
                    ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_color(rgb(theme.text_muted))
                    .truncate()
                    .child(path.display().to_string()),
            )
            .child(control)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.terminal_command_specs_summary()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_format",
                                TerminalCommandSpecsAction::Format,
                                cx,
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_example",
                                TerminalCommandSpecsAction::Example,
                                cx,
                            ))
                            .child(self.terminal_command_specs_button(
                                "settings_view.terminal.command_specs_save",
                                TerminalCommandSpecsAction::Save,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_settings_multiline_textarea_lines(
        &self,
        mut textarea: Div,
        target: WorkspaceImeTarget,
        value: &str,
        placeholder: bool,
        line_height: f32,
    ) -> Div {
        let selection = self.ime_selected_range_for_target(target);
        let theme = self.tokens.ui;
        for (line_range, line_text) in settings_multiline_line_ranges(value) {
            let (selection_range, caret_offset) =
                settings_multiline_line_selection(selection.as_ref(), &line_range);
            let line_box_height = (line_height - SETTINGS_TERMINAL_TEXTAREA_LINE_GAP).max(1.0);
            let mut line = div().min_h(px(line_box_height));
            if placeholder {
                // Browser placeholder text is not part of the editable value;
                // keep it muted and do not feed it through selection segments.
                line = line.text_color(rgb(theme.text_muted)).child(line_text);
            } else {
                // Tauri uses a real textarea, so caret/selection sit inside the
                // current visual line. Native renders line elements manually and
                // must split the shared UTF-16 IME selection per line.
                line = line.child(text_input_value_segments(
                    &self.tokens,
                    &line_text,
                    false,
                    selection_range,
                    caret_offset,
                    self.new_connection_caret_visible,
                ));
            }
            textarea = textarea.child(line);
        }
        textarea
    }

    fn terminal_command_specs_button(
        &self,
        label_key: &'static str,
        action: TerminalCommandSpecsAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Command-spec editor actions behave like Tauri Button onClick handlers:
        // disabled/loading guards live at the shared workspace boundary, not in
        // each feature listener.
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if action == TerminalCommandSpecsAction::Save {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Outline
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
            cx.listener(move |this, _event, window, cx| {
                this.handle_terminal_command_specs_action(action, window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn load_terminal_command_specs_editor_value(&self) -> String {
        let path = super::terminal_command_bar::completion::terminal_command_specs_path(
            self.settings_store.path(),
        );
        std::fs::read_to_string(path).unwrap_or_default()
    }

    fn terminal_command_specs_summary(&self) -> String {
        let built_in_count =
            super::terminal_command_bar::completion::built_in_terminal_fig_specs().len();
        let custom_count = super::terminal_command_bar::completion::user_terminal_fig_specs_count(
            self.settings_store.path(),
        );
        self.i18n
            .t("settings_view.terminal.command_specs_summary")
            .replace("{builtIn}", &built_in_count.to_string())
            .replace("{custom}", &custom_count.to_string())
    }

    fn current_terminal_command_specs_editor_value(&self) -> String {
        if self.focused_settings_input == Some(SettingsInput::TerminalCommandSpecsJson) {
            self.settings_input_draft.clone()
        } else {
            self.load_terminal_command_specs_editor_value()
        }
    }

    fn handle_terminal_command_specs_action(
        &mut self,
        action: TerminalCommandSpecsAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = SettingsInput::TerminalCommandSpecsJson;
        match action {
            TerminalCommandSpecsAction::Example => {
                let example =
                    super::terminal_command_bar::completion::terminal_command_specs_example_json();
                self.focus_settings_input(input, example, cx);
                window.focus(&self.focus_handle);
            }
            TerminalCommandSpecsAction::Format => {
                let value = self.current_terminal_command_specs_editor_value();
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(&value) {
                    Ok(pretty) => {
                        self.focus_settings_input(input, pretty, cx);
                        window.focus(&self.focus_handle);
                    }
                    Err(error) => self.push_ai_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n
                                .t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                    ),
                }
            }
            TerminalCommandSpecsAction::Save => {
                let value = self.current_terminal_command_specs_editor_value();
                match super::terminal_command_bar::completion::normalize_terminal_command_specs_json(&value) {
                    Ok(pretty) => {
                        let path =
                            super::terminal_command_bar::completion::terminal_command_specs_path(
                                self.settings_store.path(),
                            );
                        let result = path
                            .parent()
                            .map(std::fs::create_dir_all)
                            .transpose()
                            .and_then(|_| std::fs::write(&path, pretty.as_bytes()));
                        match result {
                            Ok(()) => {
                                if self.focused_settings_input == Some(input) {
                                    self.settings_input_draft = pretty;
                                }
                                self.push_ai_settings_toast(
                                    self.i18n.t("settings_view.terminal.command_specs_saved"),
                                    TerminalNoticeVariant::Success,
                                );
                                cx.notify();
                            }
                            Err(error) => self.push_ai_settings_toast(
                                error.to_string(),
                                TerminalNoticeVariant::Error,
                            ),
                        }
                    }
                    Err(error) => self.push_ai_settings_toast(
                        format!(
                            "{} {}",
                            self.i18n
                                .t("settings_view.terminal.command_specs_invalid"),
                            error
                        ),
                        TerminalNoticeVariant::Error,
                    ),
                }
            }
        }
    }

    fn in_band_transfer_number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            self.number_input(input, value.to_string(), width, cx),
            cx,
        )
    }

    fn in_band_transfer_runtime_note(&self) -> AnyElement {
        const TAURI_RUNTIME_NOTE_BORDER_ALPHA: f32 = 0.30;
        const TAURI_RUNTIME_NOTE_BACKGROUND_ALPHA: f32 = 0.10;

        // Tauri renders this as `border-amber-500/30 bg-amber-500/10 p-3 text-xs`;
        // keep the amber opacity mapping explicit instead of folding it into
        // the generic settings card row style.
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba(
                (self.tokens.ui.warning << 8) | alpha_byte(TAURI_RUNTIME_NOTE_BORDER_ALPHA),
            ))
            .bg(rgba(
                (self.tokens.ui.warning << 8) | alpha_byte(TAURI_RUNTIME_NOTE_BACKGROUND_ALPHA),
            ))
            .p(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                self.i18n
                    .t("settings_view.terminal.in_band_transfer.runtime_note"),
            )
            .into_any_element()
    }
}
