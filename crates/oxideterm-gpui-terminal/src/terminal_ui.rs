use std::{path::PathBuf, sync::Arc, time::Duration};

use gpui::{
    Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Pixels, SharedString, TextRun,
    Window, px, rgb,
};
use oxideterm_render_policy::EffectiveRenderPolicy;
use oxideterm_terminal::{
    TerminalColor, TerminalCursorShape, TerminalEncoding, TrzszTransferPolicy,
};
use oxideterm_theme::{ThemeTokens, default_tokens};

pub const MAX_HIGHLIGHT_RULES: usize = 32;
pub const MAX_HIGHLIGHT_PATTERN_LENGTH: usize = 512;

pub(crate) const DEFAULT_COLS: usize = 120;
pub(crate) const DEFAULT_ROWS: usize = 40;
pub(crate) const DEFAULT_SCROLLBACK_LINES: usize = 1000;
pub const TERMINAL_FONT: &str = oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY;
pub(crate) const TERMINAL_FONT_SIZE: f32 = 14.0;
pub(crate) const TERMINAL_LINE_HEIGHT_RATIO: f32 = 1.2;
pub(crate) const TERMINAL_CONTENT_PADDING: f32 = 0.0;
// Command marks no longer reserve a left gutter; column-zero terminal text must
// start at the pane edge.
pub(crate) const TERMINAL_COMMAND_MARK_GUTTER_WIDTH: f32 = 0.0;
pub(crate) const OXIDETERM_TERMINAL_BACKGROUND: u32 = 0x0d0f12;
pub(crate) const OXIDETERM_TERMINAL_FOREGROUND: u32 = 0xe6e8eb;
pub(crate) const SCROLLBAR_WIDTH: f32 = 10.0;
pub(crate) const SCROLLBAR_GAP: f32 = 0.0;
pub(crate) const SCROLLBAR_RESERVED_WIDTH: f32 = SCROLLBAR_WIDTH;
pub(crate) const SCROLLBAR_MIN_THUMB: f32 = 24.0;
pub(crate) const TERMINAL_TIMESTAMP_LABEL_CELLS: usize = 8;
pub(crate) const TERMINAL_TIMESTAMP_GUTTER_GAP_CELLS: f32 = 1.0;
pub(crate) const TERMINAL_SCROLL_MULTIPLIER: f32 = 1.0;
pub(crate) const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const TERMINAL_BLINK_MODE: TerminalBlinkMode = TerminalBlinkMode::On;
pub(crate) const TERMINAL_PASTE_PROTECTION: bool = true;
pub(crate) const TERMINAL_SMART_COPY: bool = true;
pub(crate) const TERMINAL_OSC52_CLIPBOARD: bool = true;
pub(crate) const TERMINAL_OSC52_CLIPBOARD_READ: bool = false;
pub(crate) const TERMINAL_COPY_ON_SELECT: bool = false;
pub(crate) const TERMINAL_MIDDLE_CLICK_PASTE: bool = false;
pub(crate) const TERMINAL_KEEP_SELECTION_ON_COPY: bool = true;
pub(crate) const TERMINAL_SELECTION_REQUIRES_SHIFT: bool = false;
pub(crate) const TERMINAL_FREE_TYPE_CURSOR_POSITIONING: bool = false;
pub(crate) const TERMINAL_FONT_LIGATURES: bool = false;
pub(crate) const TERMINAL_BIDI_ENABLED: bool = true;
pub(crate) const TERMINAL_COMMAND_MARKS_ENABLED: bool = true;
pub(crate) const TERMINAL_COMMAND_MARKS_SHOW_HOVER_ACTIONS: bool = true;

#[derive(Clone)]
pub struct TerminalUiPreferences {
    pub font_family: String,
    pub cjk_font_family: Option<String>,
    pub font_ligatures: bool,
    pub font_size: f32,
    pub line_height: f32,
    pub cursor_shape: TerminalCursorShape,
    pub cursor_blink: bool,
    pub scrollback_lines: usize,
    pub smooth_scroll: bool,
    pub paste_protection: bool,
    pub smart_copy: bool,
    pub osc52_clipboard: bool,
    pub osc52_clipboard_read: bool,
    pub copy_on_select: bool,
    pub middle_click_paste: bool,
    pub selection_requires_shift: bool,
    pub free_type_cursor_positioning: bool,
    pub bidi_enabled: bool,
    pub current_directory_awareness_enabled: bool,
    pub command_marks_enabled: bool,
    pub command_marks_user_input_observed: bool,
    pub command_marks_heuristic_detection: bool,
    pub command_marks_show_hover_actions: bool,
    pub terminal_encoding: TerminalEncoding,
    pub show_performance_overlay: bool,
    pub theme: TerminalUiTheme,
    pub render_policy: EffectiveRenderPolicy,
    pub background: Option<TerminalBackgroundPreferences>,
    pub transparent_background: bool,
    pub paste_labels: TerminalPasteLabels,
    pub command_selection_labels: TerminalCommandSelectionLabels,
    pub modem_labels: TerminalModemLabels,
    pub trzsz_labels: TerminalTrzszLabels,
    pub serial_control_labels: TerminalSerialControlLabels,
    pub notice_sink: Option<Arc<dyn Fn(TerminalNotice) + Send + Sync + 'static>>,
    pub highlight_rules: Arc<[TerminalHighlightRule]>,
    pub trzsz_policy: Option<TrzszTransferPolicy>,
}

impl Default for TerminalUiPreferences {
    fn default() -> Self {
        Self {
            font_family: TERMINAL_FONT.to_string(),
            cjk_font_family: None,
            font_ligatures: TERMINAL_FONT_LIGATURES,
            font_size: TERMINAL_FONT_SIZE,
            line_height: TERMINAL_LINE_HEIGHT_RATIO,
            cursor_shape: TerminalCursorShape::Block,
            cursor_blink: true,
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
            smooth_scroll: true,
            paste_protection: TERMINAL_PASTE_PROTECTION,
            smart_copy: TERMINAL_SMART_COPY,
            osc52_clipboard: TERMINAL_OSC52_CLIPBOARD,
            osc52_clipboard_read: TERMINAL_OSC52_CLIPBOARD_READ,
            copy_on_select: TERMINAL_COPY_ON_SELECT,
            middle_click_paste: TERMINAL_MIDDLE_CLICK_PASTE,
            selection_requires_shift: TERMINAL_SELECTION_REQUIRES_SHIFT,
            free_type_cursor_positioning: TERMINAL_FREE_TYPE_CURSOR_POSITIONING,
            bidi_enabled: TERMINAL_BIDI_ENABLED,
            current_directory_awareness_enabled: false,
            command_marks_enabled: TERMINAL_COMMAND_MARKS_ENABLED,
            command_marks_user_input_observed: false,
            command_marks_heuristic_detection: false,
            command_marks_show_hover_actions: TERMINAL_COMMAND_MARKS_SHOW_HOVER_ACTIONS,
            terminal_encoding: TerminalEncoding::Utf8,
            show_performance_overlay: false,
            theme: TerminalUiTheme::default(),
            render_policy: EffectiveRenderPolicy::quality(),
            background: None,
            transparent_background: false,
            paste_labels: TerminalPasteLabels::default(),
            command_selection_labels: TerminalCommandSelectionLabels::default(),
            modem_labels: TerminalModemLabels::default(),
            trzsz_labels: TerminalTrzszLabels::default(),
            serial_control_labels: TerminalSerialControlLabels::default(),
            notice_sink: None,
            highlight_rules: Arc::from(Vec::<TerminalHighlightRule>::new()),
            trzsz_policy: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalRenderTier {
    Boost,
    Normal,
    Idle,
}

impl TerminalRenderTier {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Boost => "B",
            Self::Normal => "N",
            Self::Idle => "I",
        }
    }

    pub(crate) fn color(self) -> u32 {
        match self {
            Self::Boost => 0x22c55e,
            Self::Normal => 0xfacc15,
            Self::Idle => 0x94a3b8,
        }
    }
}

pub(crate) fn terminal_scrollbar_x_for_viewport(viewport_width: Pixels) -> Pixels {
    // Tauri/xterm uses overviewRuler.width as right-side terminal viewport
    // chrome. Anchor the native scrollbar to that viewport edge instead of
    // deriving its x-position from the rounded PTY column count.
    px((f32::from(viewport_width) - TERMINAL_CONTENT_PADDING - SCROLLBAR_WIDTH).max(0.0))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalRenderStats {
    pub tier: TerminalRenderTier,
    pub writes_per_sec: u32,
    pub pending_bytes: usize,
}

impl Default for TerminalRenderStats {
    fn default() -> Self {
        Self {
            tier: TerminalRenderTier::Normal,
            writes_per_sec: 0,
            pending_bytes: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalNoticeVariant {
    Default,
    Success,
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct TerminalNotice {
    pub title: String,
    pub description: Option<String>,
    pub status_text: Option<String>,
    pub progress: Option<f32>,
    pub variant: TerminalNoticeVariant,
}

#[derive(Clone, Debug)]
pub struct TerminalCommandSelectionLabels {
    pub actions: String,
    pub copy: String,
    pub copy_title: String,
    pub copy_command: String,
    pub send_to_ai: String,
    pub fill_command_bar: String,
    pub insert_selection_into_command: String,
    pub replace_command_with_selection: String,
    pub find: String,
    pub select_command: String,
    pub previous_command: String,
    pub next_command: String,
    pub clear_screen: String,
}

impl Default for TerminalCommandSelectionLabels {
    fn default() -> Self {
        Self {
            actions: "Command selection actions".to_string(),
            copy: "Copy".to_string(),
            copy_title: "Copy command output".to_string(),
            copy_command: "Copy command".to_string(),
            send_to_ai: "Send to OxideSens".to_string(),
            fill_command_bar: "Fill command bar".to_string(),
            insert_selection_into_command: "Insert selection here".to_string(),
            replace_command_with_selection: "Replace command with selection".to_string(),
            find: "Find...".to_string(),
            select_command: "Select command".to_string(),
            previous_command: "Previous command".to_string(),
            next_command: "Next command".to_string(),
            clear_screen: "Clear screen".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalModemLabels {
    pub binary_transfer: String,
    pub xmodem_upload: String,
    pub xmodem_receive: String,
    pub ymodem_upload: String,
    pub ymodem_receive: String,
    pub zmodem_upload: String,
    pub zmodem_receive: String,
}

impl Default for TerminalModemLabels {
    fn default() -> Self {
        Self {
            binary_transfer: "Binary transfer".to_string(),
            xmodem_upload: "XMODEM upload".to_string(),
            xmodem_receive: "XMODEM receive".to_string(),
            ymodem_upload: "YMODEM upload".to_string(),
            ymodem_receive: "YMODEM receive".to_string(),
            zmodem_upload: "ZMODEM upload".to_string(),
            zmodem_receive: "ZMODEM receive".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalSerialControlLabels {
    pub serial: String,
    pub connected: String,
    pub disconnected: String,
    pub closed: String,
    pub port_available: String,
    pub port_missing: String,
    pub port_unknown: String,
    pub refresh: String,
    pub reconnect: String,
    pub send_break: String,
    pub dtr: String,
    pub rts: String,
    pub on: String,
    pub off: String,
    pub flow_none: String,
    pub flow_software: String,
    pub flow_hardware: String,
    pub send_mode: String,
    pub display_mode: String,
    pub line_ending: String,
    pub local_echo: String,
    pub text_mode: String,
    pub hex_mode: String,
    pub mixed_mode: String,
    pub line_ending_lf: String,
    pub line_ending_crlf: String,
    pub line_ending_cr: String,
    pub line_ending_none: String,
    pub reconnect_failed: String,
}

impl Default for TerminalSerialControlLabels {
    fn default() -> Self {
        Self {
            serial: "Serial".to_string(),
            connected: "Connected".to_string(),
            disconnected: "Disconnected".to_string(),
            closed: "Closed".to_string(),
            port_available: "Port available".to_string(),
            port_missing: "Port missing".to_string(),
            port_unknown: "Port unknown".to_string(),
            refresh: "Refresh".to_string(),
            reconnect: "Reconnect".to_string(),
            send_break: "Break".to_string(),
            dtr: "DTR".to_string(),
            rts: "RTS".to_string(),
            on: "On".to_string(),
            off: "Off".to_string(),
            flow_none: "No flow".to_string(),
            flow_software: "XON/XOFF".to_string(),
            flow_hardware: "RTS/CTS".to_string(),
            send_mode: "Send".to_string(),
            display_mode: "Display".to_string(),
            line_ending: "Line".to_string(),
            local_echo: "Echo".to_string(),
            text_mode: "Text".to_string(),
            hex_mode: "Hex".to_string(),
            mixed_mode: "Mixed".to_string(),
            line_ending_lf: "LF".to_string(),
            line_ending_crlf: "CRLF".to_string(),
            line_ending_cr: "CR".to_string(),
            line_ending_none: "Raw".to_string(),
            reconnect_failed: "Serial reconnect failed".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalTrzszLabels {
    pub select_upload_directory_title: String,
    pub select_upload_directory_description: String,
    pub select_upload_files_title: String,
    pub select_upload_files_description: String,
    pub select_download_directory_title: String,
    pub select_download_directory_description: String,
    pub cancelled_title: String,
    pub cancelled_description: String,
    pub completed_title: String,
    pub completed_description: String,
    pub failed_title: String,
    pub failed_description: String,
    pub connection_lost_title: String,
    pub connection_lost_description: String,
    pub partial_cleanup_title: String,
    pub partial_cleanup_description: String,
    pub version_mismatch_title: String,
    pub version_mismatch_description: String,
    pub path_invalid_title: String,
    pub path_invalid_description: String,
    pub symlink_not_supported_title: String,
    pub symlink_not_supported_description: String,
    pub conflict_detected_title: String,
    pub conflict_detected_description: String,
    pub directory_not_allowed_title: String,
    pub directory_not_allowed_description: String,
    pub max_file_count_title: String,
    pub max_file_count_description: String,
    pub max_total_bytes_title: String,
    pub max_total_bytes_description: String,
    pub disabled_title: String,
    pub disabled_description: String,
}

impl Default for TerminalTrzszLabels {
    fn default() -> Self {
        Self {
            select_upload_directory_title: "Select folders to upload".to_string(),
            select_upload_directory_description: "Choose local folders to send with trzsz."
                .to_string(),
            select_upload_files_title: "Select files to upload".to_string(),
            select_upload_files_description: "Choose one or more local files for this trzsz transfer."
                .to_string(),
            select_download_directory_title: "Select download location".to_string(),
            select_download_directory_description: "Choose a local folder to receive trzsz files."
                .to_string(),
            cancelled_title: "Transfer cancelled".to_string(),
            cancelled_description: "The trzsz transfer was cancelled before it completed."
                .to_string(),
            completed_title: "Transfer completed".to_string(),
            completed_description: "The trzsz transfer completed successfully.".to_string(),
            failed_title: "Transfer failed".to_string(),
            failed_description: "OxideTerm could not complete this trzsz transfer.".to_string(),
            connection_lost_title: "Transfer interrupted by connection loss".to_string(),
            connection_lost_description:
                "The SSH connection changed while the trzsz transfer was running. Reconnect and start the transfer again."
                    .to_string(),
            partial_cleanup_title: "Transfer cleanup incomplete".to_string(),
            partial_cleanup_description:
                "Temporary transfer state could not be fully cleaned up. You can keep using the terminal, but old transfer files may remain."
                    .to_string(),
            version_mismatch_title: "trzsz protocol mismatch".to_string(),
            version_mismatch_description:
                "The remote trzsz runtime is not compatible with this OxideTerm build."
                    .to_string(),
            path_invalid_title: "Download path rejected".to_string(),
            path_invalid_description:
                "OxideTerm blocked this trzsz transfer because the selected path is invalid or outside the allowed download root."
                    .to_string(),
            symlink_not_supported_title: "Symlink transfer is not supported".to_string(),
            symlink_not_supported_description:
                "The current OxideTerm trzsz bridge does not write symbolic links.".to_string(),
            conflict_detected_title: "File conflict detected".to_string(),
            conflict_detected_description:
                "A file or folder with the same name already exists at the destination.".to_string(),
            directory_not_allowed_title: "Directory transfer disabled".to_string(),
            directory_not_allowed_description:
                "The current terminal settings do not allow trzsz directory transfer.".to_string(),
            max_file_count_title: "Too many files selected".to_string(),
            max_file_count_description:
                "This transfer contains {{selected}} files, exceeding the configured limit of {{max}}."
                    .to_string(),
            max_total_bytes_title: "Transfer size limit exceeded".to_string(),
            max_total_bytes_description:
                "This transfer requires {{selected}}, exceeding the configured limit of {{max}}."
                    .to_string(),
            disabled_title: "trzsz is not enabled".to_string(),
            disabled_description:
                "The remote server started a trzsz transfer, but in-band file transfer is not enabled. Enable trzsz in Settings -> Terminal."
                    .to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalPasteLabels {
    pub title_template: String,
    pub more_lines_template: String,
    pub confirm: String,
    pub cancel: String,
    pub paste: String,
}

impl Default for TerminalPasteLabels {
    fn default() -> Self {
        Self {
            title_template: "Multiple lines detected ({{count}} lines)".to_string(),
            more_lines_template: "... {{count}} more lines".to_string(),
            confirm: "Confirm".to_string(),
            cancel: "Cancel".to_string(),
            paste: "Paste".to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalHighlightRenderMode {
    #[default]
    Background,
    Underline,
    Outline,
}

#[derive(Clone, Debug, Default)]
pub struct TerminalHighlightRule {
    pub id: String,
    pub pattern: String,
    pub is_regex: bool,
    pub case_sensitive: bool,
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub render_mode: TerminalHighlightRenderMode,
    pub enabled: bool,
    pub priority: i64,
}

#[derive(Clone, Debug)]
pub struct TerminalBackgroundPreferences {
    pub path: PathBuf,
    pub opacity: f32,
    pub blur: f32,
    pub fit: TerminalBackgroundFit,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TerminalBackgroundFit {
    Cover,
    Contain,
    Fill,
    Tile,
}

#[derive(Clone)]
pub(crate) struct TerminalUiSettings {
    pub(crate) blink_mode: TerminalBlinkMode,
    pub(crate) paste_protection: bool,
    pub(crate) smart_copy: bool,
    pub(crate) osc52_clipboard: bool,
    pub(crate) osc52_clipboard_read: bool,
    pub(crate) copy_on_select: bool,
    pub(crate) middle_click_paste: bool,
    pub(crate) keep_selection_on_copy: bool,
    pub(crate) selection_requires_shift: bool,
    pub(crate) free_type_cursor_positioning: bool,
    pub(crate) smooth_scroll: bool,
    pub(crate) bidi_enabled: bool,
    pub(crate) current_directory_awareness_enabled: bool,
    pub(crate) command_marks_enabled: bool,
    pub(crate) command_marks_user_input_observed: bool,
    pub(crate) command_marks_show_hover_actions: bool,
}

impl Default for TerminalUiSettings {
    fn default() -> Self {
        Self {
            blink_mode: TERMINAL_BLINK_MODE,
            paste_protection: TERMINAL_PASTE_PROTECTION,
            smart_copy: TERMINAL_SMART_COPY,
            osc52_clipboard: TERMINAL_OSC52_CLIPBOARD,
            osc52_clipboard_read: TERMINAL_OSC52_CLIPBOARD_READ,
            copy_on_select: TERMINAL_COPY_ON_SELECT,
            middle_click_paste: TERMINAL_MIDDLE_CLICK_PASTE,
            keep_selection_on_copy: TERMINAL_KEEP_SELECTION_ON_COPY,
            selection_requires_shift: TERMINAL_SELECTION_REQUIRES_SHIFT,
            free_type_cursor_positioning: TERMINAL_FREE_TYPE_CURSOR_POSITIONING,
            smooth_scroll: true,
            bidi_enabled: TERMINAL_BIDI_ENABLED,
            current_directory_awareness_enabled: false,
            command_marks_enabled: TERMINAL_COMMAND_MARKS_ENABLED,
            command_marks_user_input_observed: false,
            command_marks_show_hover_actions: TERMINAL_COMMAND_MARKS_SHOW_HOVER_ACTIONS,
        }
    }
}

impl TerminalUiSettings {
    pub(crate) fn from_preferences(preferences: &TerminalUiPreferences) -> Self {
        Self {
            blink_mode: if preferences.cursor_blink {
                TerminalBlinkMode::On
            } else {
                TerminalBlinkMode::Off
            },
            paste_protection: preferences.paste_protection,
            smart_copy: preferences.smart_copy,
            osc52_clipboard: preferences.osc52_clipboard,
            osc52_clipboard_read: preferences.osc52_clipboard_read,
            copy_on_select: preferences.copy_on_select,
            middle_click_paste: preferences.middle_click_paste,
            keep_selection_on_copy: TERMINAL_KEEP_SELECTION_ON_COPY,
            selection_requires_shift: preferences.selection_requires_shift,
            free_type_cursor_positioning: preferences.free_type_cursor_positioning,
            smooth_scroll: preferences.smooth_scroll,
            bidi_enabled: preferences.bidi_enabled,
            current_directory_awareness_enabled: preferences.current_directory_awareness_enabled,
            command_marks_enabled: preferences.command_marks_enabled,
            // Tauri wires manual input through an autosuggest recorder fallback;
            // GPUI enables the same user-visible fallback whenever marks are on.
            command_marks_user_input_observed: preferences.command_marks_user_input_observed
                || preferences.command_marks_enabled,
            command_marks_show_hover_actions: preferences.command_marks_show_hover_actions,
        }
    }
}

#[derive(Clone)]
pub struct TerminalUiTheme {
    // Terminal-owned overlays still use app-level UI tokens for Radix-mapped
    // surfaces such as context menus; terminal colors alone are not enough.
    pub tokens: ThemeTokens,
    pub background: u32,
    pub(crate) bell_background: u32,
    pub foreground: u32,
    pub(crate) header_foreground: u32,
}

impl Default for TerminalUiTheme {
    fn default() -> Self {
        Self::from_tokens(default_tokens())
    }
}

pub(crate) fn terminal_color_from_hex(hex: u32) -> TerminalColor {
    TerminalColor::rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

impl TerminalUiTheme {
    pub fn new(background: u32, foreground: u32, cursor: u32) -> Self {
        Self {
            tokens: default_tokens(),
            background,
            bell_background: 0x17131a,
            foreground,
            header_foreground: cursor,
        }
    }

    pub fn from_tokens(tokens: ThemeTokens) -> Self {
        Self {
            background: tokens.terminal.background,
            bell_background: 0x17131a,
            foreground: tokens.terminal.foreground,
            header_foreground: tokens.terminal.cursor,
            tokens,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalBlinkMode {
    #[allow(dead_code)]
    Off,
    #[allow(dead_code)]
    TerminalControlled,
    On,
}

#[derive(Clone)]
pub(crate) struct TerminalMetrics {
    pub(crate) font: Font,
    pub(crate) font_size: Pixels,
    pub(crate) cell_width: Pixels,
    pub(crate) line_height: Pixels,
}

impl TerminalMetrics {
    pub(crate) fn measure_with_preferences(
        window: &mut Window,
        preferences: &TerminalUiPreferences,
    ) -> Self {
        let font_size = px(preferences.font_size);
        let line_height = px(preferences.font_size * preferences.line_height);
        let font = terminal_font_with_family_and_cjk(
            &preferences.font_family,
            preferences.cjk_font_family.as_deref(),
            preferences.font_ligatures,
        );
        let font_id = window.text_system().resolve_font(&font);
        let measured_width = window
            .text_system()
            .advance(font_id, font_size, 'm')
            .map(|advance| advance.width)
            .unwrap_or_else(|_| fallback_cell_width(window, &font, font_size));

        Self {
            font,
            font_size,
            cell_width: measured_width.max(px(1.0)),
            line_height,
        }
    }

    pub(crate) fn cell_width_f32(&self) -> f32 {
        f32::from(self.cell_width)
    }

    pub(crate) fn line_height_f32(&self) -> f32 {
        f32::from(self.line_height)
    }
}

pub(crate) fn terminal_timestamp_gutter_width(metrics: &TerminalMetrics, enabled: bool) -> f32 {
    if enabled {
        (TERMINAL_TIMESTAMP_LABEL_CELLS as f32 + TERMINAL_TIMESTAMP_GUTTER_GAP_CELLS)
            * metrics.cell_width_f32()
    } else {
        0.0
    }
}

pub(crate) fn fallback_cell_width(window: &mut Window, font: &Font, font_size: Pixels) -> Pixels {
    let sample = SharedString::from("m");
    let run = TextRun {
        len: sample.len(),
        font: font.clone(),
        color: rgb(0xe6e8eb).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };

    window
        .text_system()
        .shape_line(sample, font_size, &[run], None)
        .width
}

#[cfg(test)]
pub(crate) fn terminal_font() -> Font {
    terminal_font_with_family_and_cjk(TERMINAL_FONT, None, TERMINAL_FONT_LIGATURES)
}

pub(crate) fn terminal_font_with_family_and_cjk(
    family: &str,
    cjk_family: Option<&str>,
    font_ligatures: bool,
) -> Font {
    let mut fallback_families = Vec::new();
    push_font_fallback(&mut fallback_families, family);
    if let Some(cjk_family) = cjk_family {
        push_font_fallback(&mut fallback_families, cjk_family);
    }
    for fallback in [
        oxideterm_settings::JETBRAINS_MONO_SUBSET_FAMILY,
        "JetBrainsMono Nerd Font",
        "JetBrains Mono NF (Subset)",
        "JetBrains Mono",
        "JetBrainsMonoNL Nerd Font Mono",
        oxideterm_settings::MESLO_SUBSET_FAMILY,
        "MesloLGS Nerd Font Mono",
        oxideterm_settings::MAPLE_MONO_SUBSET_FAMILY,
        "Maple Mono NF CN",
        "Symbols Nerd Font Mono",
        "Symbols Nerd Font",
        "ui-monospace",
        "SF Mono",
        "Menlo",
        "Monaco",
        "Cascadia Mono",
        "DejaVu Sans Mono",
        "Noto Sans Mono",
        "Liberation Mono",
        "Courier New",
        "Apple Color Emoji",
    ] {
        push_font_fallback(&mut fallback_families, fallback);
    }

    Font {
        family: SharedString::from(family.to_string()),
        features: terminal_font_features(font_ligatures),
        fallbacks: Some(FontFallbacks::from_fonts(fallback_families)),
        weight: FontWeight::default(),
        style: FontStyle::Normal,
    }
}

fn push_font_fallback(fallbacks: &mut Vec<String>, family: &str) {
    let family = family.trim();
    if family.is_empty() || fallbacks.iter().any(|existing| existing == family) {
        return;
    }
    fallbacks.push(family.to_string());
}

pub(crate) fn terminal_font_features(font_ligatures: bool) -> FontFeatures {
    if font_ligatures {
        // GPUI enables the font's default OpenType features when no override is supplied.
        FontFeatures::default()
    } else {
        FontFeatures::disable_ligatures()
    }
}
