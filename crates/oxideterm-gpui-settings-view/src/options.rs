use std::path::Path;

use oxideterm_gpui_ui::select::SelectAnchorId;
use oxideterm_i18n::I18n;
use oxideterm_settings::{
    AiReasoningEffort, AiThinkingStyle, AnimationSpeed, BackgroundFit, ConflictAction,
    CursorStyle as SettingsCursorStyle, FontFamily, IdeAgentMode, Language, PersistedSettings,
    TerminalEncoding, UiDensity, UpdateChannel, UpdateProxyMode, UpdateProxyProtocol,
};
use oxideterm_theme::BUILT_IN_THEMES;

use crate::{SettingsBackgroundTabIcon, SettingsSlider};

pub fn set_terminal_cursor_blink(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.cursor_blink = value;
}

pub fn set_show_fps_overlay(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.show_fps_overlay = value;
}

pub fn set_paste_protection(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.paste_protection = value;
}

pub fn set_smart_copy(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.smart_copy = value;
}

pub fn set_copy_on_select(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.copy_on_select = value;
}

pub fn set_osc52_clipboard(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.osc52_clipboard = value;
}

pub fn set_middle_click_paste(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.middle_click_paste = value;
}

pub fn set_selection_requires_shift(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.selection_requires_shift = value;
}

pub fn compact_decimal(value: f64) -> String {
    let text = format!("{value:.1}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

pub fn font_family_options() -> &'static [FontFamily] {
    &[
        FontFamily::Jetbrains,
        FontFamily::Meslo,
        FontFamily::Maple,
        FontFamily::Cascadia,
        FontFamily::Consolas,
        FontFamily::Menlo,
        FontFamily::Custom,
    ]
}

pub fn terminal_cjk_font_options() -> &'static [&'static str] {
    &[
        "",
        oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY,
        "Sarasa Fixed SC",
        "Noto Sans Mono CJK SC",
        "Noto Sans Mono CJK TC",
        "Noto Sans Mono CJK JP",
        "Noto Sans Mono CJK KR",
        "PingFang SC",
        "Hiragino Sans GB",
        "Microsoft YaHei UI",
        "Malgun Gothic",
    ]
}

pub fn terminal_encoding_options() -> &'static [TerminalEncoding] {
    &[
        TerminalEncoding::Utf8,
        TerminalEncoding::Gbk,
        TerminalEncoding::Gb18030,
        TerminalEncoding::Big5,
        TerminalEncoding::ShiftJis,
        TerminalEncoding::EucJp,
        TerminalEncoding::EucKr,
        TerminalEncoding::Windows1252,
    ]
}

pub fn cursor_style_options() -> &'static [SettingsCursorStyle] {
    &[
        SettingsCursorStyle::Block,
        SettingsCursorStyle::Underline,
        SettingsCursorStyle::Bar,
    ]
}

pub fn density_options() -> &'static [UiDensity] {
    &[
        UiDensity::Compact,
        UiDensity::Comfortable,
        UiDensity::Spacious,
    ]
}

pub fn animation_options() -> &'static [AnimationSpeed] {
    &[
        AnimationSpeed::Off,
        AnimationSpeed::Reduced,
        AnimationSpeed::Normal,
        AnimationSpeed::Fast,
    ]
}

pub fn background_fit_options() -> &'static [BackgroundFit] {
    &[
        BackgroundFit::Cover,
        BackgroundFit::Contain,
        BackgroundFit::Fill,
        BackgroundFit::Tile,
    ]
}

pub fn is_supported_background_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp" | "gif" | "bmp"
            )
        })
        .unwrap_or(false)
}

pub fn background_tab_options() -> &'static [(&'static str, &'static str, SettingsBackgroundTabIcon)]
{
    &[
        (
            "terminal",
            "settings_view.terminal.bg_tab_terminal",
            SettingsBackgroundTabIcon::Terminal,
        ),
        (
            "local_terminal",
            "settings_view.terminal.bg_tab_local",
            SettingsBackgroundTabIcon::Monitor,
        ),
        (
            "sftp",
            "settings_view.terminal.bg_tab_sftp",
            SettingsBackgroundTabIcon::FolderInput,
        ),
        (
            "forwards",
            "settings_view.terminal.bg_tab_forwards",
            SettingsBackgroundTabIcon::ArrowLeftRight,
        ),
        (
            "settings",
            "settings_view.terminal.bg_tab_settings",
            SettingsBackgroundTabIcon::Settings,
        ),
        (
            "ide",
            "settings_view.terminal.bg_tab_ide",
            SettingsBackgroundTabIcon::Code2,
        ),
        (
            "connection_monitor",
            "settings_view.terminal.bg_tab_monitor",
            SettingsBackgroundTabIcon::Activity,
        ),
        (
            "connection_pool",
            "settings_view.terminal.bg_tab_connections",
            SettingsBackgroundTabIcon::Network,
        ),
        (
            "topology",
            "settings_view.terminal.bg_tab_topology",
            SettingsBackgroundTabIcon::Network,
        ),
        (
            "file_manager",
            "settings_view.terminal.bg_tab_files",
            SettingsBackgroundTabIcon::Folder,
        ),
        (
            "session_manager",
            "settings_view.terminal.bg_tab_sessions",
            SettingsBackgroundTabIcon::ListTree,
        ),
        (
            "launcher",
            "settings_view.terminal.bg_tab_launcher",
            SettingsBackgroundTabIcon::Rocket,
        ),
        (
            "plugin_manager",
            "settings_view.terminal.bg_tab_plugins",
            SettingsBackgroundTabIcon::Puzzle,
        ),
    ]
}

pub fn theme_display_name(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub const OXIDE_THEME_IDS: &[&str] = &[
    "azurite",
    "bismuth",
    "chromium-oxide",
    "cobalt",
    "cuprite",
    "hematite",
    "malachite",
    "magnetite",
    "ochre",
    "oxide",
    "paper-oxide",
    "silver-oxide",
    "verdigris",
];

pub fn is_oxide_theme(id: &str) -> bool {
    OXIDE_THEME_IDS.contains(&id)
}

pub fn built_in_theme_exists(id: &str) -> bool {
    BUILT_IN_THEMES.iter().any(|theme| theme.id == id)
}

pub fn set_terminal_scrollback(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.scrollback = value;
}

pub fn set_buffer_max_lines(settings: &mut PersistedSettings, value: i64) {
    settings.buffer.max_lines = value;
}

pub fn set_load_shell_profile(settings: &mut PersistedSettings, value: bool) {
    settings.local_terminal.load_shell_profile = value;
}

pub fn set_oh_my_posh(settings: &mut PersistedSettings, value: bool) {
    settings.local_terminal.oh_my_posh_enabled = value;
}

pub fn set_connection_default_port(settings: &mut PersistedSettings, value: i64) {
    settings.connection_defaults.port = value;
}

pub fn set_connection_idle_timeout(settings: &mut PersistedSettings, value: i64) {
    settings.connection_pool.idle_timeout_secs = value;
}

pub fn sftp_concurrent_options() -> &'static [i64] {
    &[1, 2, 3, 4, 5, 6, 8, 10]
}

pub fn sftp_directory_parallelism_options() -> &'static [i64] {
    &[1, 2, 3, 4, 5, 6, 8, 10, 12, 16]
}

pub fn sftp_transfer_count_label(i18n: &I18n, count: i64) -> String {
    let key = if count == 1 {
        "settings_view.sftp.transfer_count_one"
    } else {
        "settings_view.sftp.transfer_count_other"
    };
    i18n.t(key).replace("{{count}}", &count.to_string())
}

pub fn set_reconnect_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.reconnect.enabled = value;
}

pub fn set_reconnect_max_attempts(settings: &mut PersistedSettings, value: i64) {
    settings.reconnect.max_attempts = value;
}

pub fn set_reconnect_base_delay(settings: &mut PersistedSettings, value: i64) {
    settings.reconnect.base_delay_ms = value;
}

pub fn set_reconnect_max_delay(settings: &mut PersistedSettings, value: i64) {
    settings.reconnect.max_delay_ms = value;
}

pub fn set_sftp_concurrent(settings: &mut PersistedSettings, value: i64) {
    settings.sftp.max_concurrent_transfers = value;
}

pub fn set_sftp_directory_parallelism(settings: &mut PersistedSettings, value: i64) {
    settings.sftp.directory_parallelism = value;
}

pub fn set_sftp_speed_limit_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.sftp.speed_limit_enabled = value;
}

pub fn set_sftp_speed_limit_kbps(settings: &mut PersistedSettings, value: i64) {
    settings.sftp.speed_limit_kbps = value;
}

pub fn set_ide_auto_save(settings: &mut PersistedSettings, value: bool) {
    settings.ide.auto_save = value;
}

pub fn set_ide_word_wrap(settings: &mut PersistedSettings, value: bool) {
    settings.ide.word_wrap = value;
}

pub fn set_ide_font_size(settings: &mut PersistedSettings, value: i64) {
    settings.ide.font_size = Some(value);
}

pub fn set_ide_line_height_percent(settings: &mut PersistedSettings, value: i64) {
    settings.ide.line_height = Some(value as f64 / 100.0);
}

pub fn set_ai_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.ai.enabled = value;
}

pub fn set_ai_enabled_confirmed(settings: &mut PersistedSettings, value: bool) {
    settings.ai.enabled_confirmed = value;
}

pub fn set_ai_context_max_chars(settings: &mut PersistedSettings, value: i64) {
    settings.ai.context_max_chars = value;
}

pub fn set_ai_context_lines(settings: &mut PersistedSettings, value: i64) {
    settings.ai.context_visible_lines = value;
}

pub fn set_ai_context_source_ide(settings: &mut PersistedSettings, value: bool) {
    settings.ai.context_sources.ide = value;
}

pub fn set_ai_context_source_sftp(settings: &mut PersistedSettings, value: bool) {
    settings.ai.context_sources.sftp = value;
}

pub fn set_ai_memory_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.ai.memory.enabled = value;
}

pub fn set_ai_tool_use_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.ai.tool_use.enabled = value;
}

pub fn set_ai_tool_use_max_rounds(settings: &mut PersistedSettings, value: i64) {
    settings.ai.tool_use.max_rounds = Some(value);
}

pub fn set_ai_tool_use_max_calls_per_round(settings: &mut PersistedSettings, value: i64) {
    settings.ai.tool_use.max_calls_per_round = Some(value);
}

pub fn set_command_bar_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_bar.enabled = value;
}

pub fn set_command_bar_legacy_toolbar(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_bar.show_legacy_toolbar = value;
}

pub fn set_quick_commands_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_bar.quick_commands_enabled = value;
}

pub fn set_quick_commands_confirm(settings: &mut PersistedSettings, value: bool) {
    settings
        .terminal
        .command_bar
        .quick_commands_confirm_before_run = value;
}

pub fn set_quick_commands_toast(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_bar.quick_commands_show_toast = value;
}

pub fn set_autosuggest_local_history(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.autosuggest.local_shell_history = value;
}

pub fn set_command_marks_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_marks.enabled = value;
}

pub fn set_command_marks_hover_actions(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.command_marks.show_hover_actions = value;
}

pub fn set_in_band_transfer_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.in_band_transfer.enabled = value;
}

pub fn set_in_band_transfer_allow_directory(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.in_band_transfer.allow_directory = value;
}

pub fn set_in_band_transfer_max_chunk_bytes(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.in_band_transfer.max_chunk_bytes = value;
}

pub fn set_in_band_transfer_max_file_count(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.in_band_transfer.max_file_count = value;
}

pub fn set_in_band_transfer_max_total_bytes(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.in_band_transfer.max_total_bytes = value;
}

pub fn set_in_band_transfer_max_total_mb(settings: &mut PersistedSettings, value: i64) {
    settings.terminal.in_band_transfer.max_total_bytes = value * 1024 * 1024;
}

pub fn set_terminal_background_enabled(settings: &mut PersistedSettings, value: bool) {
    settings.terminal.background_enabled = value;
}

pub fn settings_slider_anchor_id(slider: SettingsSlider) -> SelectAnchorId {
    match slider {
        SettingsSlider::TerminalFontSize => SelectAnchorId::SettingsTerminalFontSizeSlider,
        SettingsSlider::AppearanceBorderRadius => {
            SelectAnchorId::SettingsAppearanceBorderRadiusSlider
        }
        SettingsSlider::AppearanceBackgroundOpacity => {
            SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider
        }
        SettingsSlider::AppearanceBackgroundBlur => {
            SelectAnchorId::SettingsAppearanceBackgroundBlurSlider
        }
    }
}

pub fn language_options() -> [Language; 11] {
    [
        Language::De,
        Language::En,
        Language::EsEs,
        Language::FrFr,
        Language::It,
        Language::Ko,
        Language::PtBr,
        Language::Vi,
        Language::Ja,
        Language::ZhCn,
        Language::ZhTw,
    ]
}

pub fn cycle_update_channel(settings: &mut PersistedSettings) {
    settings.general.update_channel = match settings.general.update_channel {
        UpdateChannel::Stable => UpdateChannel::Beta,
        UpdateChannel::Beta => UpdateChannel::GpuiPreview,
        UpdateChannel::GpuiPreview => UpdateChannel::Stable,
    };
}

pub fn cycle_sftp_conflict(settings: &mut PersistedSettings) {
    settings.sftp.conflict_action = match settings.sftp.conflict_action {
        ConflictAction::Ask => ConflictAction::Overwrite,
        ConflictAction::Overwrite => ConflictAction::Skip,
        ConflictAction::Skip => ConflictAction::Rename,
        ConflictAction::Rename => ConflictAction::Ask,
    };
}

pub fn cycle_ide_agent_mode(settings: &mut PersistedSettings) {
    settings.ide.agent_mode = match settings.ide.agent_mode {
        IdeAgentMode::Ask => IdeAgentMode::Enabled,
        IdeAgentMode::Enabled => IdeAgentMode::Disabled,
        IdeAgentMode::Disabled => IdeAgentMode::Ask,
    };
}

pub fn cycle_ai_thinking(settings: &mut PersistedSettings) {
    settings.ai.thinking_style = match settings.ai.thinking_style {
        AiThinkingStyle::Detailed => AiThinkingStyle::Compact,
        AiThinkingStyle::Compact => AiThinkingStyle::Detailed,
    };
}

pub fn cycle_ai_reasoning(settings: &mut PersistedSettings) {
    settings.ai.reasoning_effort = match settings.ai.reasoning_effort {
        AiReasoningEffort::None => AiReasoningEffort::Minimal,
        AiReasoningEffort::Minimal => AiReasoningEffort::Low,
        AiReasoningEffort::Low => AiReasoningEffort::Medium,
        AiReasoningEffort::Medium => AiReasoningEffort::High,
        AiReasoningEffort::High => AiReasoningEffort::Xhigh,
        AiReasoningEffort::Xhigh => AiReasoningEffort::Auto,
        AiReasoningEffort::Auto => AiReasoningEffort::None,
    };
}

pub fn update_channel_label(channel: UpdateChannel, i18n: &I18n) -> String {
    match channel {
        UpdateChannel::Stable => i18n.t("settings_view.help.channel_stable"),
        UpdateChannel::Beta => i18n.t("settings_view.help.channel_beta"),
        UpdateChannel::GpuiPreview => i18n.t("settings_view.help.channel_gpui_preview"),
    }
}

pub fn update_proxy_mode_label(mode: UpdateProxyMode, i18n: &I18n) -> String {
    match mode {
        UpdateProxyMode::Direct => i18n.t("settings_view.help.update_proxy_mode_direct"),
        UpdateProxyMode::System => i18n.t("settings_view.help.update_proxy_mode_system"),
        UpdateProxyMode::Custom => i18n.t("settings_view.help.update_proxy_mode_custom"),
    }
}

pub fn update_proxy_protocol_label(protocol: UpdateProxyProtocol, i18n: &I18n) -> String {
    match protocol {
        UpdateProxyProtocol::Http => i18n.t("settings_view.help.update_proxy_protocol_http"),
        UpdateProxyProtocol::Https => i18n.t("settings_view.help.update_proxy_protocol_https"),
        UpdateProxyProtocol::Socks5 => i18n.t("settings_view.help.update_proxy_protocol_socks5"),
    }
}

pub fn terminal_encoding_label(encoding: TerminalEncoding) -> String {
    match encoding {
        TerminalEncoding::Utf8 => "UTF-8",
        TerminalEncoding::Gbk => "GBK",
        TerminalEncoding::Gb18030 => "GB18030",
        TerminalEncoding::Big5 => "Big5",
        TerminalEncoding::ShiftJis => "Shift_JIS",
        TerminalEncoding::EucJp => "EUC-JP",
        TerminalEncoding::EucKr => "EUC-KR",
        TerminalEncoding::Windows1252 => "Windows-1252",
    }
    .to_string()
}

pub fn cursor_style_label(style: SettingsCursorStyle, i18n: &I18n) -> String {
    match style {
        SettingsCursorStyle::Block => i18n.t("settings_view.terminal.cursor_block"),
        SettingsCursorStyle::Underline => i18n.t("settings_view.terminal.cursor_underline"),
        SettingsCursorStyle::Bar => i18n.t("settings_view.terminal.cursor_bar"),
    }
}

pub fn background_fit_label(fit: BackgroundFit, i18n: &I18n) -> String {
    match fit {
        BackgroundFit::Cover => i18n.t("settings_view.terminal.bg_fit_cover"),
        BackgroundFit::Contain => i18n.t("settings_view.terminal.bg_fit_contain"),
        BackgroundFit::Fill => i18n.t("settings_view.terminal.bg_fit_fill"),
        BackgroundFit::Tile => i18n.t("settings_view.terminal.bg_fit_tile"),
    }
}
