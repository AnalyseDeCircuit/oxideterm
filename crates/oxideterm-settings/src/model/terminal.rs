#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeneralSettings {
    pub language: Language,
    pub update_channel: UpdateChannel,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: Language::ZhCn,
            update_channel: UpdateChannel::GpuiPreview,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAutosuggestSettings {
    pub local_shell_history: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalAutosuggestSettings {
    fn default() -> Self {
        Self {
            local_shell_history: true,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandBarSettings {
    pub enabled: bool,
    pub show_legacy_toolbar: bool,
    pub git_status: bool,
    pub smart_completion: bool,
    pub quick_commands_enabled: bool,
    pub quick_commands_confirm_before_run: bool,
    pub quick_commands_show_toast: bool,
    pub focus_handoff_commands: Vec<String>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalCommandBarSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_legacy_toolbar: false,
            git_status: true,
            smart_completion: true,
            quick_commands_enabled: true,
            quick_commands_confirm_before_run: false,
            quick_commands_show_toast: true,
            focus_handoff_commands: [
                "vim", "nvim", "vi", "nano", "emacs", "less", "more", "top", "htop", "btop",
                "yazi", "ranger", "lf", "lazygit", "tmux", "screen", "ssh", "python", "node",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandMarksSettings {
    pub enabled: bool,
    pub user_input_observed: bool,
    pub heuristic_detection: bool,
    pub show_hover_actions: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalCommandMarksSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            user_input_observed: false,
            heuristic_detection: false,
            show_hover_actions: true,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InBandTransferSettings {
    pub enabled: bool,
    pub provider: String,
    pub allow_directory: bool,
    pub max_chunk_bytes: i64,
    pub max_file_count: i64,
    pub max_total_bytes: i64,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for InBandTransferSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "trzsz".to_string(),
            allow_directory: true,
            max_chunk_bytes: 1024 * 1024,
            max_file_count: 1024,
            max_total_bytes: 10 * 1024 * 1024 * 1024,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalGraphicsSettings {
    pub enabled: bool,
    pub sixel: bool,
    pub iterm2_inline: bool,
    pub kitty: bool,
    pub pixel_limit: i64,
    pub storage_limit_mb: i64,
    pub show_placeholder: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalGraphicsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sixel: true,
            iterm2_inline: true,
            kitty: true,
            pixel_limit: 16_777_216,
            storage_limit_mb: 16,
            show_placeholder: true,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalUnicodeSettings {
    pub bidi_enabled: bool,
    pub rtl_debug_overlay: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalUnicodeSettings {
    fn default() -> Self {
        Self {
            bidi_enabled: true,
            rtl_debug_overlay: false,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub theme: String,
    pub font_family: FontFamily,
    pub custom_font_family: String,
    pub font_size: i64,
    pub line_height: f64,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub scrollback: i64,
    pub renderer: RendererType,
    pub terminal_encoding: TerminalEncoding,
    pub adaptive_renderer: AdaptiveRendererMode,
    pub show_fps_overlay: bool,
    pub paste_protection: bool,
    pub smart_copy: bool,
    pub osc52_clipboard: bool,
    pub copy_on_select: bool,
    pub middle_click_paste: bool,
    pub selection_requires_shift: bool,
    pub autosuggest: TerminalAutosuggestSettings,
    pub command_bar: TerminalCommandBarSettings,
    pub command_marks: TerminalCommandMarksSettings,
    pub background_enabled: bool,
    pub background_image: Option<String>,
    pub background_opacity: f64,
    pub background_blur: i64,
    pub background_fit: BackgroundFit,
    pub background_enabled_tabs: Vec<String>,
    pub highlight_rules: Vec<HighlightRule>,
    pub in_band_transfer: InBandTransferSettings,
    pub graphics: TerminalGraphicsSettings,
    pub unicode: TerminalUnicodeSettings,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            font_family: FontFamily::Jetbrains,
            custom_font_family: String::new(),
            font_size: 14,
            line_height: 1.2,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            scrollback: DEFAULT_TERMINAL_SCROLLBACK,
            renderer: RendererType::default(),
            terminal_encoding: TerminalEncoding::Utf8,
            adaptive_renderer: AdaptiveRendererMode::Auto,
            show_fps_overlay: false,
            paste_protection: true,
            smart_copy: true,
            osc52_clipboard: true,
            copy_on_select: false,
            middle_click_paste: false,
            selection_requires_shift: false,
            autosuggest: TerminalAutosuggestSettings::default(),
            command_bar: TerminalCommandBarSettings::default(),
            command_marks: TerminalCommandMarksSettings::default(),
            background_enabled: true,
            background_image: None,
            background_opacity: 0.15,
            background_blur: 0,
            background_fit: BackgroundFit::Cover,
            background_enabled_tabs: vec!["terminal".to_string(), "local_terminal".to_string()],
            highlight_rules: Vec::new(),
            in_band_transfer: InBandTransferSettings::default(),
            graphics: TerminalGraphicsSettings::default(),
            unicode: TerminalUnicodeSettings::default(),
            extra: ExtraFields::new(),
        }
    }
}
