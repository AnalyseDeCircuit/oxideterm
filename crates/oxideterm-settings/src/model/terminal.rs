#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeneralSettings {
    pub language: Language,
    pub update_channel: UpdateChannel,
    #[serde(default)]
    pub update_proxy: UpdateProxySettings,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: Language::ZhCn,
            update_channel: UpdateChannel::default(),
            update_proxy: UpdateProxySettings::default(),
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
    pub git_status: bool,
    #[serde(default = "default_command_bar_project_tasks")]
    pub project_tasks: bool,
    #[serde(default)]
    pub current_directory_awareness: bool,
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
            git_status: true,
            project_tasks: true,
            current_directory_awareness: false,
            smart_completion: true,
            quick_commands_enabled: true,
            quick_commands_confirm_before_run: false,
            quick_commands_show_toast: true,
            focus_handoff_commands: [
                "btop",
                "emacs",
                "fzf",
                "htop",
                "lazydocker",
                "lazygit",
                "less",
                "man",
                "micro",
                "nano",
                "nvim",
                "ranger",
                "screen",
                "ssh",
                "tig",
                "tmux",
                "top",
                "vi",
                "vim",
                "yazi",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            extra: ExtraFields::new(),
        }
    }
}

fn default_command_bar_project_tasks() -> bool {
    true
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

fn default_terminal_smooth_scroll() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub theme: String,
    pub font_family: FontFamily,
    pub custom_font_family: String,
    #[serde(default)]
    pub cjk_font_family: String,
    pub font_size: i64,
    pub line_height: f64,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub scrollback: i64,
    #[serde(default = "default_terminal_smooth_scroll")]
    pub smooth_scroll: bool,
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
    #[serde(default)]
    pub free_type_cursor_positioning: bool,
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
            cjk_font_family: String::new(),
            font_size: 14,
            line_height: 1.2,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            scrollback: DEFAULT_TERMINAL_SCROLLBACK,
            smooth_scroll: true,
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
            free_type_cursor_positioning: false,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_settings_default_smooth_scroll_when_missing() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        value.as_object_mut().unwrap().remove("smoothScroll");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();

        assert!(settings.smooth_scroll);
    }

    #[test]
    fn terminal_settings_default_free_type_cursor_positioning_when_missing() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("freeTypeCursorPositioning");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();

        assert!(!settings.free_type_cursor_positioning);
    }

    #[test]
    fn command_bar_settings_default_current_directory_awareness_when_missing() {
        let mut value = serde_json::to_value(TerminalCommandBarSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("currentDirectoryAwareness");

        let settings: TerminalCommandBarSettings = serde_json::from_value(value).unwrap();

        assert!(!settings.current_directory_awareness);
    }

    #[test]
    fn command_bar_settings_default_project_tasks_when_missing() {
        let mut value = serde_json::to_value(TerminalCommandBarSettings::default()).unwrap();
        value.as_object_mut().unwrap().remove("projectTasks");

        let settings: TerminalCommandBarSettings = serde_json::from_value(value).unwrap();

        assert!(settings.project_tasks);
    }
}
