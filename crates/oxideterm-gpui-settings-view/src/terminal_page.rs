use gpui::{
    AnyElement, App, MouseButton, MouseDownEvent, MouseMoveEvent, div, prelude::*, px, rgb,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    select::{SelectAnchorId, select_anchor_probe},
    slider::{SliderView, slider},
};
use oxideterm_settings::PersistedSettings;
use oxideterm_settings_model::SettingsSectionId;

use crate::{
    SettingsInput, SettingsSelect, SettingsSlider,
    components::{
        SettingsCalloutTone, SettingsControls, SettingsWindowRequest, TerminalPreviewFont,
    },
    labels::*,
    options::*,
};

pub const SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH: f32 = 300.0;

pub fn terminal_session_log_root_directory(
    settings_path: &std::path::Path,
    configured_directory: Option<&str>,
) -> std::path::PathBuf {
    if let Some(directory) = configured_directory
        .map(str::trim)
        .filter(|path| !path.is_empty())
    {
        let directory = std::path::PathBuf::from(directory);
        if directory.is_absolute() {
            return directory;
        }
    }

    // A path synchronized from another operating system may not be absolute here.
    settings_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("logs")
        .join("terminal")
}

pub enum TerminalSettingsSection {
    Font {
        custom_cjk: bool,
        preview: TerminalPreviewFont,
    },
    DisplayBehavior,
    Cursor,
    CommandMarks,
    Buffer,
    Clipboard,
    MouseLinks,
    Keyboard,
    InBandTransfer,
    Xmodem,
    SessionLogging {
        default_directory: String,
    },
}

impl SettingsControls<'_> {
    pub fn terminal_settings_section(
        &self,
        section: TerminalSettingsSection,
        cx: &App,
    ) -> AnyElement {
        let settings = self.settings;
        match section {
            TerminalSettingsSection::Font {
                custom_cjk,
                preview,
            } => {
                let custom_cjk =
                    custom_cjk || cjk_font_is_custom(&settings.terminal.cjk_font_family);
                let mut rows = vec![self.select_setting_row(
                    "settings_view.terminal.font_family",
                    "settings_view.terminal.font_family_hint",
                    SettingsSelect::TerminalFontFamily,
                    font_family_label(settings.terminal.font_family),
                    self.tokens.metrics.settings_select_width,
                    cx,
                )];
                if settings.terminal.font_family == oxideterm_settings::FontFamily::Custom {
                    rows.push(self.setting_row(
                        "settings_view.terminal.custom_font_stack",
                        "settings_view.terminal.custom_font_stack_hint",
                        self.settings_text_input_control(
                            SettingsInput::TerminalCustomFontFamily,
                            settings.terminal.custom_font_family.clone(),
                            "'Sarasa Fixed SC', 'Fira Code', monospace".to_string(),
                            SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH,
                            cx,
                        ),
                        cx,
                    ));
                }
                rows.push(self.card_separator());
                rows.push(self.select_setting_row(
                    "settings_view.terminal.cjk_font_family",
                    "settings_view.terminal.cjk_font_family_hint",
                    SettingsSelect::TerminalCjkFontFamily,
                    if custom_cjk {
                        self.i18n.t("settings_view.terminal.cjk_font_custom")
                    } else {
                        terminal_cjk_font_label(&settings.terminal.cjk_font_family, &self.i18n)
                    },
                    self.tokens.metrics.settings_select_width,
                    cx,
                ));
                if custom_cjk {
                    rows.push(self.setting_row(
                        "settings_view.terminal.custom_cjk_font",
                        "settings_view.terminal.custom_cjk_font_hint",
                        self.settings_text_input_control(
                            SettingsInput::TerminalCjkFontFamily,
                            settings.terminal.cjk_font_family.clone(),
                            "PingFang SC".to_string(),
                            SETTINGS_TERMINAL_CUSTOM_FONT_INPUT_WIDTH,
                            cx,
                        ),
                        cx,
                    ));
                }
                rows.extend([
                    self.card_separator(),
                    self.decimal_row(
                        "settings_view.terminal.font_weight",
                        "settings_view.terminal.font_weight_hint",
                        SettingsInput::TerminalFontWeight,
                        settings.terminal.font_weight.to_string(),
                        cx,
                    ),
                    self.terminal_preview(preview),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.font_ligatures",
                        "settings_view.terminal.font_ligatures_hint",
                        settings.terminal.font_ligatures,
                        set_font_ligatures,
                        cx,
                    ),
                    self.card_separator(),
                    self.font_size_row(settings, cx),
                    self.card_separator(),
                    self.decimal_row(
                        "settings_view.terminal.character_spacing",
                        "settings_view.terminal.character_spacing_hint",
                        SettingsInput::TerminalCharacterSpacing,
                        compact_decimal(settings.terminal.character_spacing),
                        cx,
                    ),
                    self.card_separator(),
                    self.decimal_row(
                        "settings_view.terminal.line_height",
                        "settings_view.terminal.line_height_hint",
                        SettingsInput::TerminalLineHeight,
                        compact_decimal(settings.terminal.line_height),
                        cx,
                    ),
                ]);
                self.settings_card(
                    "settings_view.terminal.font",
                    "settings_view.terminal.font_hint",
                    rows,
                )
            }
            TerminalSettingsSection::DisplayBehavior => self.settings_card(
                "settings_view.terminal.display_behavior",
                "settings_view.terminal.display_behavior_hint",
                vec![
                    self.decimal_row(
                        "settings_view.terminal.padding_horizontal",
                        "settings_view.terminal.padding_horizontal_hint",
                        SettingsInput::TerminalPaddingHorizontal,
                        settings.terminal.padding_horizontal.to_string(),
                        cx,
                    ),
                    self.card_separator(),
                    self.decimal_row(
                        "settings_view.terminal.padding_vertical",
                        "settings_view.terminal.padding_vertical_hint",
                        SettingsInput::TerminalPaddingVertical,
                        settings.terminal.padding_vertical.to_string(),
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.smooth_scroll",
                        "settings_view.terminal.smooth_scroll_hint",
                        settings.terminal.smooth_scroll,
                        set_terminal_smooth_scroll,
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
                ],
            ),
            TerminalSettingsSection::Cursor => self.settings_card(
                "settings_view.terminal.cursor",
                "",
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
                    self.bool_row(
                        "settings_view.terminal.cursor_blink",
                        "settings_view.terminal.cursor_blink_hint",
                        settings.terminal.cursor_blink,
                        set_terminal_cursor_blink,
                        cx,
                    ),
                ],
            ),
            TerminalSettingsSection::CommandMarks => self.settings_card(
                "settings_view.terminal.command_marks",
                "",
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
                        "settings_view.terminal.command_folding",
                        "settings_view.terminal.command_folding_hint",
                        settings.terminal.command_marks.folding_enabled,
                        set_command_folding_enabled,
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
            TerminalSettingsSection::Buffer => self.settings_card(
                "settings_view.terminal.buffer",
                "",
                vec![
                    self.number_row(
                        "settings_view.terminal.scrollback",
                        "settings_view.terminal.scrollback_hint",
                        SettingsInput::TerminalScrollback,
                        settings.terminal.scrollback,
                        cx,
                    ),
                    self.card_separator(),
                    self.bool_row(
                        "settings_view.terminal.highlight_tab_on_new_output",
                        "settings_view.terminal.highlight_tab_on_new_output_hint",
                        settings.terminal.highlight_tab_on_new_output,
                        set_highlight_tab_on_new_output,
                        cx,
                    ),
                ],
            ),
            TerminalSettingsSection::Clipboard => self.terminal_input_settings_card(
                SettingsSectionId::TerminalInputClipboard,
                settings,
                cx,
            ),
            TerminalSettingsSection::MouseLinks => self.terminal_input_settings_card(
                SettingsSectionId::TerminalInputMouseLinks,
                settings,
                cx,
            ),
            TerminalSettingsSection::Keyboard => self.terminal_input_settings_card(
                SettingsSectionId::TerminalInputKeyboard,
                settings,
                cx,
            ),
            TerminalSettingsSection::InBandTransfer => self.settings_card(
                "settings_view.terminal.in_band_transfer.title",
                "",
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
                    self.card_separator(),
                    self.in_band_transfer_runtime_note(),
                ],
            ),
            TerminalSettingsSection::Xmodem => self.settings_card(
                "settings_view.terminal.xmodem.title",
                "",
                vec![self.select_setting_row(
                    "settings_view.terminal.xmodem.block_size",
                    "settings_view.terminal.xmodem.block_size_hint",
                    SettingsSelect::XmodemBlockSize,
                    xmodem_block_size_label(settings.terminal.xmodem_block_size, &self.i18n),
                    self.tokens.metrics.settings_select_width,
                    cx,
                )],
            ),
            TerminalSettingsSection::SessionLogging { default_directory } => {
                let configured_directory = settings
                    .terminal
                    .session_log
                    .directory
                    .as_deref()
                    .unwrap_or_default();
                let directory_control = div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.settings_text_input_control(
                        SettingsInput::TerminalSessionLogDirectory,
                        configured_directory,
                        default_directory,
                        420.0,
                        cx,
                    ))
                    .child(self.session_log_directory_button(
                        "settings_view.general.change",
                        ButtonVariant::Outline,
                        || SettingsWindowRequest::PickSessionLogDirectory,
                    ))
                    .when(settings.terminal.session_log.directory.is_some(), |row| {
                        row.child(self.session_log_directory_button(
                            "settings_view.general.reset_to_default",
                            ButtonVariant::Ghost,
                            || {
                                SettingsWindowRequest::Edit(Box::new(|settings| {
                                    settings.terminal.session_log.directory = None
                                }))
                            },
                        ))
                    })
                    .into_any_element();
                self.settings_card(
                    "settings_view.terminal.session_log_title",
                    "settings_view.terminal.session_log_description",
                    vec![
                        self.bool_row(
                            "settings_view.terminal.session_log_automatic",
                            "settings_view.terminal.session_log_automatic_hint",
                            settings.terminal.session_log.automatic,
                            set_terminal_session_log_automatic,
                            cx,
                        ),
                        self.card_separator(),
                        self.setting_row(
                            "settings_view.terminal.session_log_directory",
                            "settings_view.terminal.session_log_directory_hint",
                            directory_control,
                            cx,
                        ),
                        self.card_separator(),
                        self.setting_row(
                            "settings_view.terminal.session_log_directory_template",
                            "settings_view.terminal.session_log_directory_template_hint",
                            self.settings_text_input_control(
                                SettingsInput::TerminalSessionLogDirectoryTemplate,
                                &settings.terminal.session_log.directory_template,
                                "{session}/{date}".to_string(),
                                420.0,
                                cx,
                            ),
                            cx,
                        ),
                        self.card_separator(),
                        self.setting_row(
                            "settings_view.terminal.session_log_file_name_template",
                            "settings_view.terminal.session_log_file_name_template_hint",
                            self.settings_text_input_control(
                                SettingsInput::TerminalSessionLogFileNameTemplate,
                                &settings.terminal.session_log.file_name_template,
                                "{date}_{time}_{protocol}_{session}.log".to_string(),
                                420.0,
                                cx,
                            ),
                            cx,
                        ),
                        self.card_separator(),
                        self.select_setting_row(
                            "settings_view.terminal.session_log_file_mode",
                            "settings_view.terminal.session_log_file_mode_hint",
                            SettingsSelect::TerminalSessionLogFileMode,
                            terminal_session_log_file_mode_label(
                                settings.terminal.session_log.file_mode,
                                &self.i18n,
                            ),
                            self.tokens.metrics.settings_select_width,
                            cx,
                        ),
                        self.card_separator(),
                        self.setting_row(
                            "settings_view.terminal.session_log_content_template",
                            "settings_view.terminal.session_log_content_template_hint",
                            self.settings_text_input_control(
                                SettingsInput::TerminalSessionLogContentTemplate,
                                &settings.terminal.session_log.content_template,
                                "[{timestamp}] {text}".to_string(),
                                420.0,
                                cx,
                            ),
                            cx,
                        ),
                        self.card_separator(),
                        self.bool_row(
                            "settings_view.terminal.session_log_control_sequences",
                            "settings_view.terminal.session_log_control_sequences_hint",
                            settings.terminal.session_log.include_control_sequences,
                            set_terminal_session_log_include_control_sequences,
                            cx,
                        ),
                        self.card_separator(),
                        self.number_row(
                            "settings_view.terminal.session_log_retention_days",
                            "settings_view.terminal.session_log_retention_days_hint",
                            SettingsInput::TerminalSessionLogRetentionDays,
                            settings.terminal.session_log.retention_days,
                            cx,
                        ),
                        self.card_separator(),
                        self.number_row(
                            "settings_view.terminal.session_log_max_file_size",
                            "settings_view.terminal.session_log_max_file_size_hint",
                            SettingsInput::TerminalSessionLogMaxFileSizeMib,
                            settings.terminal.session_log.max_file_size_mib,
                            cx,
                        ),
                    ],
                )
            }
        }
    }

    fn terminal_input_settings_card(
        &self,
        section: SettingsSectionId,
        settings: &PersistedSettings,
        cx: &App,
    ) -> AnyElement {
        match section {
            SettingsSectionId::TerminalInputClipboard => {
                let mut rows = Vec::new();
                rows.push(self.bool_row(
                    "settings_view.terminal.paste_protection",
                    "settings_view.terminal.paste_protection_hint",
                    settings.terminal.paste_protection,
                    set_paste_protection,
                    cx,
                ));
                rows.push(self.bool_row(
                    "settings_view.terminal.osc52_clipboard",
                    "settings_view.terminal.osc52_clipboard_hint",
                    settings.terminal.osc52_clipboard,
                    set_osc52_clipboard,
                    cx,
                ));
                rows.push(self.bool_row(
                    "settings_view.terminal.osc52_clipboard_read",
                    "settings_view.terminal.osc52_clipboard_read_hint",
                    settings.terminal.osc52_clipboard_read,
                    set_osc52_clipboard_read,
                    cx,
                ));
                if !cfg!(target_os = "macos") {
                    rows.push(self.bool_row(
                        "settings_view.terminal.smart_copy",
                        "settings_view.terminal.smart_copy_hint",
                        settings.terminal.smart_copy,
                        set_smart_copy,
                        cx,
                    ));
                }
                rows.push(self.bool_row(
                    "settings_view.terminal.copy_on_select",
                    "settings_view.terminal.copy_on_select_hint",
                    settings.terminal.copy_on_select,
                    set_copy_on_select,
                    cx,
                ));
                rows.push(self.bool_row(
                    "settings_view.terminal.middle_click_paste",
                    "settings_view.terminal.middle_click_paste_hint",
                    settings.terminal.middle_click_paste,
                    set_middle_click_paste,
                    cx,
                ));
                rows.push(self.bool_row(
                    "settings_view.terminal.right_click_paste",
                    "settings_view.terminal.right_click_paste_hint",
                    settings.terminal.right_click_paste,
                    set_right_click_paste,
                    cx,
                ));
                self.settings_card(
                    "settings_view.terminal.input_clipboard",
                    "",
                    self.separated_settings_rows(rows),
                )
            }
            SettingsSectionId::TerminalInputMouseLinks => {
                let rows = vec![
                    self.bool_row(
                        "settings_view.terminal.selection_requires_shift",
                        "settings_view.terminal.selection_requires_shift_hint",
                        settings.terminal.selection_requires_shift,
                        set_selection_requires_shift,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.open_links_with_modifier",
                        "settings_view.terminal.open_links_with_modifier_hint",
                        settings.terminal.open_links_with_modifier,
                        set_open_links_with_modifier,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.detect_file_paths_as_links",
                        "settings_view.terminal.detect_file_paths_as_links_hint",
                        settings.terminal.detect_file_paths_as_links,
                        set_detect_file_paths_as_links,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.timestamp_hover",
                        "settings_view.terminal.timestamp_hover_hint",
                        settings.terminal.timestamp_hover,
                        set_timestamp_hover,
                        cx,
                    ),
                ];
                self.settings_card(
                    "settings_view.terminal.input_mouse_links",
                    "",
                    self.separated_settings_rows(rows),
                )
            }
            SettingsSectionId::TerminalInputKeyboard => {
                let rows = vec![
                    self.bool_row(
                        "settings_view.terminal.free_type_mode",
                        "settings_view.terminal.free_type_mode_hint",
                        settings.terminal.free_type_mode,
                        set_free_type_mode,
                        cx,
                    ),
                    self.bool_row(
                        "settings_view.terminal.autosuggest_enabled",
                        "settings_view.terminal.autosuggest_enabled_hint",
                        settings.terminal.autosuggest.enabled,
                        set_terminal_autosuggest_enabled,
                        cx,
                    ),
                    self.select_setting_row(
                        "settings_view.terminal.backspace_sequence",
                        "settings_view.terminal.backspace_sequence_hint",
                        SettingsSelect::TerminalBackspaceSequence,
                        terminal_backspace_sequence_label(settings.terminal.backspace_sequence)
                            .to_string(),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                    self.select_setting_row(
                        "settings_view.terminal.delete_sequence",
                        "settings_view.terminal.delete_sequence_hint",
                        SettingsSelect::TerminalDeleteSequence,
                        terminal_delete_sequence_label(settings.terminal.delete_sequence)
                            .to_string(),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                ];
                self.settings_card(
                    "settings_view.terminal.input_keyboard",
                    "",
                    self.separated_settings_rows(rows),
                )
            }
            _ => div().into_any_element(),
        }
    }

    fn font_size_row(&self, settings: &PersistedSettings, cx: &App) -> AnyElement {
        let slider_view = SliderView {
            min: 8.0,
            max: 32.0,
            value: settings.terminal.font_size as f32,
            disabled: false,
        };
        let dispatch = self.dispatch.clone();
        let control = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .w(px(self.tokens.metrics.settings_slider_width))
                    .child(select_anchor_probe(
                        SelectAnchorId::SettingsTerminalFontSizeSlider,
                        slider(&self.tokens, slider_view)
                            .cursor_pointer()
                            .on_mouse_down(MouseButton::Left, {
                                let dispatch = dispatch.clone();
                                move |event: &MouseDownEvent, window, cx| {
                                    dispatch(
                                        SettingsWindowRequest::BeginSlider(
                                            SettingsSlider::TerminalFontSize,
                                            f32::from(event.position.x),
                                        ),
                                        window,
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            })
                            .on_mouse_up(MouseButton::Left, {
                                let dispatch = dispatch.clone();
                                move |_, window, cx| {
                                    dispatch(SettingsWindowRequest::FinishSlider, window, cx);
                                    cx.stop_propagation();
                                }
                            })
                            .on_mouse_move({
                                let dispatch = dispatch.clone();
                                move |event: &MouseMoveEvent, window, cx| {
                                    dispatch(
                                        SettingsWindowRequest::MoveSlider(event.clone()),
                                        window,
                                        cx,
                                    );
                                }
                            }),
                        move |anchor, window, cx| {
                            dispatch(SettingsWindowRequest::SliderAnchor(anchor), window, cx);
                        },
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(4.0))
                    .child(self.number_input(
                        SettingsInput::TerminalFontSize,
                        settings.terminal.font_size.to_string(),
                        self.tokens.metrics.settings_font_size_input_width,
                        cx,
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child("px"),
                    ),
            )
            .into_any_element();

        self.setting_row(
            "settings_view.terminal.font_size",
            "settings_view.terminal.font_size_hint",
            control,
            cx,
        )
    }

    fn in_band_transfer_number_row(
        &self,
        label_key: &str,
        hint_key: &str,
        input: SettingsInput,
        value: i64,
        width: f32,
        cx: &App,
    ) -> AnyElement {
        self.setting_row(
            label_key,
            hint_key,
            self.number_input(input, value.to_string(), width, cx),
            cx,
        )
    }

    fn in_band_transfer_runtime_note(&self) -> AnyElement {
        self.settings_callout(
            SettingsCalloutTone::Warning,
            Some("lucide/info.svg"),
            div()
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(
                    self.i18n
                        .t("settings_view.terminal.in_band_transfer.runtime_note"),
                )
                .into_any_element(),
        )
    }
    fn session_log_directory_button(
        &self,
        label: &'static str,
        variant: ButtonVariant,
        request: impl Fn() -> SettingsWindowRequest + 'static,
    ) -> AnyElement {
        let dispatch = self.dispatch.clone();
        toolbar_button(
            self.tokens,
            self.i18n.t(label),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            dispatch(request(), window, cx);
            cx.stop_propagation();
        })
        .into_any_element()
    }
}
