use gpui::{SharedString, Window, px};
use oxideterm_render_policy::EffectiveRenderPolicy;
use oxideterm_settings::{
    TerminalBackspaceSequence, TerminalDeleteSequence, TerminalRightClickAction,
    TerminalSemanticScheme,
};
use oxideterm_terminal::{TerminalCursorShape, TerminalEncoding, TrzszTransferPolicy};
use oxideterm_terminal_semantic::{CompiledSemanticScheme, SemanticShellDialect};
use std::{sync::Arc, time::Duration};

pub use oxideterm_gpui_terminal_view::metrics::{
    ResolvedTerminalFont, TERMINAL_FONT, resolve_terminal_font, terminal_letter_spacing_for_font,
    terminal_line_height_for_font,
};
pub use oxideterm_gpui_terminal_view::theme::{
    MAX_HIGHLIGHT_PATTERN_LENGTH, MAX_HIGHLIGHT_RULES, TerminalHighlightMatchScope,
    TerminalHighlightRenderMode, TerminalHighlightRule, TerminalHighlightRuleSetOverride,
    TerminalUiTheme, resolved_terminal_semantic_scheme, terminal_semantic_color,
    terminal_semantic_line_band, terminal_semantic_variant_color,
};
pub(crate) use oxideterm_gpui_terminal_view::{metrics::*, theme::*};

use crate::{
    command_facts::SharedTerminalCommandHistory,
    session_log::{TerminalSessionLogContext, TerminalSessionLogOptions},
};

pub(crate) const DEFAULT_COLS: usize = 120;
pub(crate) const DEFAULT_ROWS: usize = 40;
pub(crate) const DEFAULT_SCROLLBACK_LINES: usize = 10_000;
pub(crate) const SCROLLBAR_HIT_WIDTH: f32 = 16.0;
pub(crate) const SCROLLBAR_GAP: f32 = 0.0;
pub(crate) const SCROLLBAR_HIDE_DELAY: Duration = Duration::from_secs(1);
pub(crate) const TERMINAL_SCROLL_MULTIPLIER: f32 = 1.0;
pub(crate) const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const TERMINAL_PASTE_PROTECTION: bool = true;
pub(crate) const TERMINAL_SMART_COPY: bool = true;
pub(crate) const TERMINAL_OSC52_CLIPBOARD: bool = true;
pub(crate) const TERMINAL_OSC52_CLIPBOARD_READ: bool = false;
pub(crate) const TERMINAL_COPY_ON_SELECT: bool = false;
pub(crate) const TERMINAL_MIDDLE_CLICK_PASTE: bool = false;
pub(crate) const TERMINAL_OPEN_LINKS_WITH_MODIFIER: bool = true;
pub(crate) const TERMINAL_DETECT_FILE_PATHS_AS_LINKS: bool = true;
pub(crate) const TERMINAL_KEEP_SELECTION_ON_COPY: bool = true;
pub(crate) const TERMINAL_SELECTION_REQUIRES_SHIFT: bool = false;
pub(crate) const TERMINAL_FREE_TYPE_MODE: bool = false;
pub(crate) const TERMINAL_BACKSPACE_SEQUENCE: TerminalBackspaceSequence =
    TerminalBackspaceSequence::Delete;
pub(crate) const TERMINAL_DELETE_SEQUENCE: TerminalDeleteSequence =
    TerminalDeleteSequence::Csi3Tilde;
pub(crate) const TERMINAL_BIDI_ENABLED: bool = true;
pub(crate) const TERMINAL_COMMAND_MARKS_ENABLED: bool = true;
pub(crate) const TERMINAL_COMMAND_MARKS_SHOW_HOVER_ACTIONS: bool = true;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalBellStyle {
    Off,
    #[default]
    SoftFlash,
    EdgeGlow,
    CursorPulse,
    HeaderStrip,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalBellPolicy {
    #[default]
    Inherit,
    Silent,
    TabHighlight,
}

#[derive(Clone)]
pub struct TerminalUiPreferences {
    pub font_family: String,
    pub cjk_font_family: Option<String>,
    pub font_ligatures: bool,
    pub font_size: f32,
    pub font_weight: f32,
    pub character_spacing: f32,
    pub line_height: f32,
    pub padding_horizontal: f32,
    pub padding_vertical: f32,
    pub cursor_shape: TerminalCursorShape,
    pub cursor_blink: bool,
    pub scrollback_lines: usize,
    pub smooth_scroll: bool,
    pub optical_bloom: bool,
    pub crt_effect: bool,
    pub bell_style: TerminalBellStyle,
    pub bell_policy: TerminalBellPolicy,
    pub paste_protection: bool,
    pub smart_copy: bool,
    pub osc52_clipboard: bool,
    pub osc52_clipboard_read: bool,
    pub copy_on_select: bool,
    pub middle_click_paste: bool,
    pub right_click_action: TerminalRightClickAction,
    pub open_links_with_modifier: bool,
    pub detect_file_paths_as_links: bool,
    pub timestamp_hover: bool,
    pub semantic_coloring: bool,
    pub selection_highlighting: bool,
    pub semantic_scheme: Arc<CompiledSemanticScheme>,
    pub semantic_shell: SemanticShellDialect,
    pub selection_requires_shift: bool,
    pub free_type_mode: bool,
    pub autosuggest_enabled: bool,
    pub backspace_sequence: TerminalBackspaceSequence,
    pub delete_sequence: TerminalDeleteSequence,
    pub bidi_enabled: bool,
    pub current_directory_awareness_enabled: bool,
    pub command_marks_enabled: bool,
    pub command_folding_enabled: bool,
    pub command_marks_user_input_observed: bool,
    pub command_marks_heuristic_detection: bool,
    pub command_marks_show_hover_actions: bool,
    pub command_history: SharedTerminalCommandHistory,
    pub terminal_encoding: TerminalEncoding,
    pub show_performance_overlay: bool,
    pub theme: TerminalUiTheme,
    pub render_policy: EffectiveRenderPolicy,
    pub background: Option<TerminalBackgroundPreferences>,
    pub transparent_background: bool,
    pub processing_failed_message: String,
    pub reconnect_label: String,
    pub reconnect_hint: String,
    pub archive_link_hint: String,
    pub archive_link_modifier_hint: String,
    pub reconnecting_label: String,
    pub paste_labels: TerminalPasteLabels,
    pub kitty_file_transmission_labels: TerminalKittyFileTransmissionLabels,
    pub autosuggest_labels: TerminalAutosuggestLabels,
    pub command_selection_labels: TerminalCommandSelectionLabels,
    pub modem_labels: TerminalModemLabels,
    pub trzsz_labels: TerminalTrzszLabels,
    pub serial_control_labels: TerminalSerialControlLabels,
    pub tmux_labels: TerminalTmuxLabels,
    pub control_bar_expand_label: String,
    pub control_bar_collapse_label: String,
    pub session_log_options: Option<TerminalSessionLogOptions>,
    pub session_log_automatic: bool,
    pub session_log_labels: TerminalSessionLogLabels,
    pub notice_sink: Option<Arc<dyn Fn(TerminalNotice) + Send + Sync + 'static>>,
    pub highlight_rules: Arc<[TerminalHighlightRule]>,
    pub trzsz_policy: Option<TrzszTransferPolicy>,
    pub xmodem_block_size: oxideterm_settings::XmodemBlockSize,
}

#[derive(Clone, Default)]
pub struct TerminalUiPreferenceOverrides {
    pub terminal_encoding: Option<TerminalEncoding>,
    pub backspace_sequence: Option<TerminalBackspaceSequence>,
    pub delete_sequence: Option<TerminalDeleteSequence>,
    pub semantic_scheme: Option<Arc<CompiledSemanticScheme>>,
    pub semantic_scheme_id: Option<String>,
    pub highlight_rules: Option<Arc<[TerminalHighlightRule]>>,
    pub highlight_rule_set_id: Option<String>,
    pub semantic_shell: Option<SemanticShellDialect>,
    // Retain the local shell identity so settings refreshes can resolve its Scheme again.
    pub local_shell_id: Option<String>,
    pub session_log_available: Option<bool>,
    pub session_log_automatic: Option<bool>,
    pub session_log_context: Option<TerminalSessionLogContext>,
    pub bell_policy: Option<TerminalBellPolicy>,
}

impl TerminalUiPreferenceOverrides {
    pub fn apply_to(&self, preferences: &mut TerminalUiPreferences) {
        // These protocol-facing values belong to the terminal session. Apply
        // them after application defaults so later settings refreshes cannot
        // erase a saved host's explicit behavior.
        if let Some(terminal_encoding) = self.terminal_encoding {
            preferences.terminal_encoding = terminal_encoding;
        }
        if let Some(backspace_sequence) = self.backspace_sequence {
            preferences.backspace_sequence = backspace_sequence;
        }
        if let Some(delete_sequence) = self.delete_sequence {
            preferences.delete_sequence = delete_sequence;
        }
        if let Some(semantic_scheme) = &self.semantic_scheme {
            preferences.semantic_scheme = semantic_scheme.clone();
        }
        if let Some(highlight_rules) = &self.highlight_rules {
            preferences.highlight_rules = highlight_rules.clone();
        }
        if let Some(semantic_shell) = self.semantic_shell {
            preferences.semantic_shell = semantic_shell;
        }
        if self.session_log_available == Some(false) {
            preferences.session_log_options = None;
        }
        if let Some(automatic) = self.session_log_automatic {
            preferences.session_log_automatic = automatic;
        }
        if let Some(context) = &self.session_log_context
            && let Some(options) = preferences.session_log_options.as_mut()
        {
            options.context = context.clone();
        }
        if let Some(bell_policy) = self.bell_policy {
            preferences.bell_policy = bell_policy;
        }
    }
}

impl Default for TerminalUiPreferences {
    fn default() -> Self {
        Self {
            font_family: TERMINAL_FONT.to_string(),
            cjk_font_family: None,
            font_ligatures: TERMINAL_FONT_LIGATURES,
            font_size: TERMINAL_FONT_SIZE,
            font_weight: TERMINAL_FONT_WEIGHT,
            character_spacing: TERMINAL_CHARACTER_SPACING,
            line_height: TERMINAL_LINE_HEIGHT_RATIO,
            padding_horizontal: oxideterm_settings::DEFAULT_TERMINAL_PADDING_HORIZONTAL as f32,
            padding_vertical: oxideterm_settings::DEFAULT_TERMINAL_PADDING_VERTICAL as f32,
            cursor_shape: TerminalCursorShape::Block,
            cursor_blink: true,
            scrollback_lines: DEFAULT_SCROLLBACK_LINES,
            smooth_scroll: true,
            optical_bloom: false,
            crt_effect: false,
            bell_style: TerminalBellStyle::default(),
            bell_policy: TerminalBellPolicy::default(),
            paste_protection: TERMINAL_PASTE_PROTECTION,
            smart_copy: TERMINAL_SMART_COPY,
            osc52_clipboard: TERMINAL_OSC52_CLIPBOARD,
            osc52_clipboard_read: TERMINAL_OSC52_CLIPBOARD_READ,
            copy_on_select: TERMINAL_COPY_ON_SELECT,
            middle_click_paste: TERMINAL_MIDDLE_CLICK_PASTE,
            right_click_action: TerminalRightClickAction::default(),
            open_links_with_modifier: TERMINAL_OPEN_LINKS_WITH_MODIFIER,
            detect_file_paths_as_links: TERMINAL_DETECT_FILE_PATHS_AS_LINKS,
            timestamp_hover: true,
            // Match persisted settings so standalone terminal views remain opt-in as well.
            semantic_coloring: false,
            selection_highlighting: false,
            semantic_scheme: resolved_terminal_semantic_scheme(
                TerminalSemanticScheme::default(),
                None,
            ),
            semantic_shell: SemanticShellDialect::Auto,
            selection_requires_shift: TERMINAL_SELECTION_REQUIRES_SHIFT,
            free_type_mode: TERMINAL_FREE_TYPE_MODE,
            autosuggest_enabled: true,
            backspace_sequence: TERMINAL_BACKSPACE_SEQUENCE,
            delete_sequence: TERMINAL_DELETE_SEQUENCE,
            bidi_enabled: TERMINAL_BIDI_ENABLED,
            current_directory_awareness_enabled: false,
            command_marks_enabled: TERMINAL_COMMAND_MARKS_ENABLED,
            command_folding_enabled: false,
            command_marks_user_input_observed: false,
            command_marks_heuristic_detection: false,
            command_marks_show_hover_actions: TERMINAL_COMMAND_MARKS_SHOW_HOVER_ACTIONS,
            command_history: SharedTerminalCommandHistory::default(),
            terminal_encoding: TerminalEncoding::Utf8,
            show_performance_overlay: false,
            theme: TerminalUiTheme::default(),
            render_policy: EffectiveRenderPolicy::quality(),
            background: None,
            transparent_background: false,
            processing_failed_message:
                "Terminal processing stopped. Reconnect this session to continue.".into(),
            reconnect_label: String::new(),
            reconnect_hint: String::new(),
            archive_link_hint: String::new(),
            archive_link_modifier_hint: String::new(),
            reconnecting_label: String::new(),
            paste_labels: TerminalPasteLabels::default(),
            kitty_file_transmission_labels: TerminalKittyFileTransmissionLabels::default(),
            autosuggest_labels: TerminalAutosuggestLabels::default(),
            command_selection_labels: TerminalCommandSelectionLabels::default(),
            modem_labels: TerminalModemLabels::default(),
            trzsz_labels: TerminalTrzszLabels::default(),
            serial_control_labels: TerminalSerialControlLabels::default(),
            tmux_labels: TerminalTmuxLabels::default(),
            control_bar_expand_label: "Show controls".into(),
            control_bar_collapse_label: "Hide controls".into(),
            session_log_options: None,
            session_log_automatic: false,
            session_log_labels: TerminalSessionLogLabels::default(),
            notice_sink: None,
            highlight_rules: Arc::from(Vec::<TerminalHighlightRule>::new()),
            trzsz_policy: None,
            xmodem_block_size: oxideterm_settings::XmodemBlockSize::default(),
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalRenderStats {
    pub tier: TerminalRenderTier,
    pub writes_per_sec: u32,
    pub pending_bytes: usize,
    pub drain_micros: u64,
    pub drain_p95_micros: u64,
    pub drained_bytes: usize,
    pub max_data_chunk_bytes: usize,
    pub output_processing_micros: u64,
    pub terminal_lock_wait_micros: u64,
    pub snapshot_micros: u64,
    pub layout_micros: u64,
    pub paint_micros: u64,
    pub layout_cache_hit_percent: u8,
    pub scroll_snapshot_micros: u64,
    pub scroll_snapshot_count: u64,
    pub search_micros: u64,
    pub image_prepare_micros: u64,
    pub input_latency_p50_micros: u64,
    pub input_latency_p95_micros: u64,
    pub input_latency_p99_micros: u64,
}

impl Default for TerminalRenderStats {
    fn default() -> Self {
        Self {
            tier: TerminalRenderTier::Normal,
            writes_per_sec: 0,
            pending_bytes: 0,
            drain_micros: 0,
            drain_p95_micros: 0,
            drained_bytes: 0,
            max_data_chunk_bytes: 0,
            output_processing_micros: 0,
            terminal_lock_wait_micros: 0,
            snapshot_micros: 0,
            layout_micros: 0,
            paint_micros: 0,
            layout_cache_hit_percent: 0,
            scroll_snapshot_micros: 0,
            scroll_snapshot_count: 0,
            search_micros: 0,
            image_prepare_micros: 0,
            input_latency_p50_micros: 0,
            input_latency_p95_micros: 0,
            input_latency_p99_micros: 0,
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
pub struct TerminalAutosuggestLabels {
    pub selection_shortcut: Option<String>,
    pub history_source: String,
    pub matches: String,
    pub navigation_hint: String,
    pub dismiss_hint: String,
}

impl Default for TerminalAutosuggestLabels {
    fn default() -> Self {
        Self {
            selection_shortcut: None,
            history_source: "history".to_string(),
            matches: "{{count}} matches".into(),
            navigation_hint: "{{select}} select · {{run}} run · Click fill".into(),
            dismiss_hint: "{{dismiss}} close · {{remove}} remove".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalCommandSelectionLabels {
    pub actions: String,
    pub open_link: String,
    pub copy: String,
    pub copy_title: String,
    pub copy_command: String,
    pub shell_ide: String,
    pub shell_ide_preparing: String,
    pub send_to_ai: String,
    pub fill_command_bar: String,
    pub insert_selection_into_command: String,
    pub replace_command_with_selection: String,
    pub find: String,
    pub manage_triggers: String,
    pub select_command: String,
    pub collapse_output: String,
    pub expand_output: String,
    pub folded_output: String,
    pub folded_exit_code: String,
    pub previous_command: String,
    pub next_command: String,
    pub clear_screen: String,
    pub clear_screen_shortcut: Option<String>,
}

impl Default for TerminalCommandSelectionLabels {
    fn default() -> Self {
        Self {
            actions: "Command selection actions".to_string(),
            open_link: "Open link".to_string(),
            copy: "Copy".to_string(),
            copy_title: "Copy command output".to_string(),
            copy_command: "Copy command".to_string(),
            shell_ide: "Convert to IDE".to_string(),
            shell_ide_preparing: "Starting IDE file service; terminal input is paused…".into(),
            send_to_ai: "Send to OxideSens".to_string(),
            fill_command_bar: "Fill command bar".to_string(),
            insert_selection_into_command: "Insert selection here".to_string(),
            replace_command_with_selection: "Replace command with selection".to_string(),
            find: "Find...".to_string(),
            manage_triggers: "Manage triggers...".to_string(),
            select_command: "Select command".to_string(),
            collapse_output: "Collapse output".to_string(),
            expand_output: "Expand output".to_string(),
            folded_output: "{{count}} output lines collapsed".to_string(),
            folded_exit_code: "Exit code {{code}}".to_string(),
            previous_command: "Previous command".to_string(),
            next_command: "Next command".to_string(),
            clear_screen: "Clear screen".to_string(),
            clear_screen_shortcut: Some("Ctrl+L".to_string()),
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
    pub timeout: String,
    pub protocol_error: String,
    pub file_error: String,
    pub file_too_large: String,
    pub buffer_overflow: String,
    pub worker_stopped: String,
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
            timeout: "The peer did not respond in time. Start its transfer program, then retry."
                .to_string(),
            protocol_error: "The peer sent invalid or unexpected transfer data.".to_string(),
            file_error:
                "Unable to read or write the transfer file. Check permissions and free space."
                    .to_string(),
            file_too_large: "The file exceeds this protocol's size limit.".to_string(),
            buffer_overflow: "The peer sent more data than the transfer buffer can hold."
                .to_string(),
            worker_stopped: "The transfer worker stopped unexpectedly. Please retry.".to_string(),
        }
    }
}

/// A toolbar chip owned by the embedder: the pane only renders the pushed
/// specs and reports clicks by id, without interpreting their semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalToolbarActionSpec {
    pub id: SharedString,
    pub label: String,
    /// False renders the chip disabled; `disabled_hint` explains why.
    pub enabled: bool,
    /// Marks the chip active (the pane currently exhibits this action's mode).
    pub active: bool,
    /// Tooltip shown while disabled; ignored when enabled.
    pub disabled_hint: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TerminalSerialControlLabels {
    pub serial: String,
    pub ble: String,
    pub connected: String,
    pub connecting: String,
    pub disconnected: String,
    pub closed: String,
    pub port_available: String,
    pub port_missing: String,
    pub port_unknown: String,
    pub refresh: String,
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
    pub output_line_ending: String,
    pub local_echo: String,
    pub text_mode: String,
    pub hex_mode: String,
    pub mixed_mode: String,
    pub line_ending_lf: String,
    pub line_ending_crlf: String,
    pub line_ending_cr: String,
    pub line_ending_none: String,
}

#[derive(Clone, Debug)]
pub struct TerminalTmuxLabels {
    pub tmux: String,
    pub initializing: String,
    pub previous_window: String,
    pub next_window: String,
    pub new_session: String,
    pub close_session: String,
    pub new_window: String,
    pub split_horizontal: String,
    pub split_vertical: String,
    pub close_pane: String,
    pub close_window: String,
    pub detach: String,
    pub resize_left: String,
    pub resize_right: String,
    pub resize_up: String,
    pub resize_down: String,
    pub cancel_mode: String,
    pub command_failed: String,
    pub rename_session: String,
    pub rename_window: String,
    pub command: String,
    pub command_prompt: String,
    pub command_placeholder: String,
    pub name_placeholder: String,
    pub confirm: String,
    pub cancel: String,
}

impl Default for TerminalTmuxLabels {
    fn default() -> Self {
        Self {
            tmux: "tmux".to_string(),
            initializing: "Initializing".to_string(),
            previous_window: "Previous window".to_string(),
            next_window: "Next window".to_string(),
            new_session: "New session".to_string(),
            close_session: "Close session".to_string(),
            new_window: "New window".to_string(),
            split_horizontal: "Split horizontally".to_string(),
            split_vertical: "Split vertically".to_string(),
            close_pane: "Close pane".to_string(),
            close_window: "Close window".to_string(),
            detach: "Detach".to_string(),
            resize_left: "Resize left".to_string(),
            resize_right: "Resize right".to_string(),
            resize_up: "Resize up".to_string(),
            resize_down: "Resize down".to_string(),
            cancel_mode: "Exit mode".to_string(),
            command_failed: "tmux command failed".to_string(),
            rename_session: "Rename session".to_string(),
            rename_window: "Rename window".to_string(),
            command: "Command".to_string(),
            command_prompt: "Run tmux command".to_string(),
            command_placeholder: "Enter a tmux command".to_string(),
            name_placeholder: "Enter a name".to_string(),
            confirm: "Confirm".to_string(),
            cancel: "Cancel".to_string(),
        }
    }
}

impl Default for TerminalSerialControlLabels {
    fn default() -> Self {
        Self {
            serial: "Serial".to_string(),
            ble: "BLE".to_string(),
            connected: "Connected".to_string(),
            connecting: "Connecting".to_string(),
            disconnected: "Disconnected".to_string(),
            closed: "Closed".to_string(),
            port_available: "Port available".to_string(),
            port_missing: "Port missing".to_string(),
            port_unknown: "Port unknown".to_string(),
            refresh: "Refresh".to_string(),
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
            line_ending: "TX line".to_string(),
            output_line_ending: "RX line".to_string(),
            local_echo: "Echo".to_string(),
            text_mode: "Text".to_string(),
            hex_mode: "Hex".to_string(),
            mixed_mode: "Mixed".to_string(),
            line_ending_lf: "LF".to_string(),
            line_ending_crlf: "CRLF".to_string(),
            line_ending_cr: "CR".to_string(),
            line_ending_none: "Raw".to_string(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TerminalSessionLogLabels {
    pub start_failed: String,
    pub write_failed: String,
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

/// Localized copy for the per-session Kitty file-access decision.
#[derive(Clone, Debug)]
pub struct TerminalKittyFileTransmissionLabels {
    pub title: String,
    pub description: String,
    pub cancel: String,
    pub allow: String,
    pub allowed_title: String,
    pub allowed_description: String,
    pub failed_title: String,
    pub failed_description: String,
}

impl Default for TerminalKittyFileTransmissionLabels {
    fn default() -> Self {
        Self {
            title: "Allow Kitty local resources for this session?".to_string(),
            description: "Programs in this terminal, including remote programs, will be able to read local files and shared memory for images. Only allow sessions you trust. Permission ends on close or reconnect; retry blocked transfers after allowing.".to_string(),
            cancel: "Deny".to_string(),
            allow: "Allow for this session".to_string(),
            allowed_title: "Kitty file access allowed for this session".to_string(),
            allowed_description: "Local image resources are allowed until this session closes or reconnects. Retry the transfer.".to_string(),
            failed_title: "Could not enable Kitty file access".to_string(),
            failed_description: "OxideTerm could not authorize local resources for this session.".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalPasteLabels {
    pub edit_title: String,
    pub strip_fence: String,
    pub undo: String,
    pub redo: String,
    pub editor_menu: oxideterm_gpui_editor::EditorContextMenuLabels,
    pub cancel: String,
    pub paste: String,
}

impl Default for TerminalPasteLabels {
    fn default() -> Self {
        Self {
            edit_title: "Edit before pasting".into(),
            strip_fence: "Remove code block markers".into(),
            undo: "Undo".into(),
            redo: "Redo".into(),
            editor_menu: oxideterm_gpui_editor::EditorContextMenuLabels::default(),
            cancel: "Cancel".to_string(),
            paste: "Paste".to_string(),
        }
    }
}

pub use oxideterm_gpui_background::{
    BackgroundFit as TerminalBackgroundFit, BackgroundPreferences as TerminalBackgroundPreferences,
};

#[derive(Clone)]
pub(crate) struct TerminalUiSettings {
    pub(crate) blink_mode: TerminalBlinkMode,
    pub(crate) paste_protection: bool,
    pub(crate) smart_copy: bool,
    pub(crate) osc52_clipboard: bool,
    pub(crate) osc52_clipboard_read: bool,
    pub(crate) copy_on_select: bool,
    pub(crate) middle_click_paste: bool,
    pub(crate) right_click_action: TerminalRightClickAction,
    pub(crate) open_links_with_modifier: bool,
    pub(crate) detect_file_paths_as_links: bool,
    pub(crate) keep_selection_on_copy: bool,
    pub(crate) selection_requires_shift: bool,
    pub(crate) free_type_mode: bool,
    pub(crate) backspace_sequence: TerminalBackspaceSequence,
    pub(crate) delete_sequence: TerminalDeleteSequence,
    pub(crate) smooth_scroll: bool,
    pub(crate) bidi_enabled: bool,
    pub(crate) current_directory_awareness_enabled: bool,
    pub(crate) command_marks_enabled: bool,
    pub(crate) command_marks_user_input_observed: bool,
    pub(crate) command_marks_show_hover_actions: bool,
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
            right_click_action: preferences.right_click_action,
            open_links_with_modifier: preferences.open_links_with_modifier,
            detect_file_paths_as_links: preferences.detect_file_paths_as_links,
            keep_selection_on_copy: TERMINAL_KEEP_SELECTION_ON_COPY,
            selection_requires_shift: preferences.selection_requires_shift,
            free_type_mode: preferences.free_type_mode,
            backspace_sequence: preferences.backspace_sequence,
            delete_sequence: preferences.delete_sequence,
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

pub(crate) use oxideterm_gpui_terminal_view::input::TerminalBlinkMode;

pub(crate) trait TerminalMetricsExt {
    fn measure_with_preferences(window: &mut Window, preferences: &TerminalUiPreferences) -> Self;
}

impl TerminalMetricsExt for TerminalMetrics {
    fn measure_with_preferences(window: &mut Window, preferences: &TerminalUiPreferences) -> Self {
        let font_size = px(preferences.font_size);
        let font = resolve_terminal_font(
            window.text_system(),
            &preferences.font_family,
            preferences.cjk_font_family.as_deref(),
            preferences.font_ligatures,
            preferences.font_weight,
        )
        .font;

        Self::measure(
            window,
            font,
            font_size,
            preferences.line_height,
            preferences.character_spacing,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_overrides_replace_only_terminal_protocol_defaults() {
        let mut preferences = TerminalUiPreferences::default();
        let original_font_family = preferences.font_family.clone();

        TerminalUiPreferenceOverrides {
            terminal_encoding: Some(TerminalEncoding::Gb18030),
            backspace_sequence: Some(TerminalBackspaceSequence::ControlH),
            delete_sequence: Some(TerminalDeleteSequence::Delete),
            ..TerminalUiPreferenceOverrides::default()
        }
        .apply_to(&mut preferences);

        assert_eq!(preferences.terminal_encoding, TerminalEncoding::Gb18030);
        assert_eq!(
            preferences.backspace_sequence,
            TerminalBackspaceSequence::ControlH
        );
        assert_eq!(preferences.delete_sequence, TerminalDeleteSequence::Delete);
        assert_eq!(preferences.font_family, original_font_family);
    }
}
