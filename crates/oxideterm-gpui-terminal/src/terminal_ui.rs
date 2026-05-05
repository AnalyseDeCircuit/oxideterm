use std::time::Duration;

use gpui::{
    Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Pixels, SharedString, TextRun,
    Window, px, rgb,
};
use oxideterm_render_policy::EffectiveRenderPolicy;
use oxideterm_terminal::{
    TerminalColor, TerminalCursorShape, TerminalEncoding, TerminalLifecycle, TerminalProcessInfo,
};

pub(crate) const DEFAULT_COLS: usize = 120;
pub(crate) const DEFAULT_ROWS: usize = 40;
pub const TERMINAL_FONT: &str = "JetBrainsMono Nerd Font";
pub(crate) const TERMINAL_FONT_SIZE: f32 = 14.0;
pub(crate) const TERMINAL_LINE_HEIGHT_RATIO: f32 = 1.2;
pub(crate) const TERMINAL_CONTENT_PADDING: f32 = 0.0;
pub(crate) const OXIDETERM_TERMINAL_BACKGROUND: u32 = 0x0d0f12;
pub(crate) const OXIDETERM_TERMINAL_FOREGROUND: u32 = 0xe6e8eb;
pub(crate) const SCROLLBAR_WIDTH: f32 = 3.0;
pub(crate) const SCROLLBAR_GAP: f32 = 6.0;
pub(crate) const SCROLLBAR_MIN_THUMB: f32 = 24.0;
pub(crate) const TERMINAL_SCROLL_MULTIPLIER: f32 = 1.0;
pub(crate) const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
pub(crate) const TERMINAL_BLINK_MODE: TerminalBlinkMode = TerminalBlinkMode::On;
pub(crate) const TERMINAL_COPY_ON_SELECT: bool = false;
pub(crate) const TERMINAL_KEEP_SELECTION_ON_COPY: bool = true;
pub(crate) const TERMINAL_BIDI_ENABLED: bool = true;

#[derive(Clone)]
pub struct TerminalUiPreferences {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub cursor_shape: TerminalCursorShape,
    pub cursor_blink: bool,
    pub copy_on_select: bool,
    pub bidi_enabled: bool,
    pub terminal_encoding: TerminalEncoding,
    pub theme: TerminalUiTheme,
    pub render_policy: EffectiveRenderPolicy,
}

impl Default for TerminalUiPreferences {
    fn default() -> Self {
        Self {
            font_family: TERMINAL_FONT.to_string(),
            font_size: TERMINAL_FONT_SIZE,
            line_height: TERMINAL_LINE_HEIGHT_RATIO,
            cursor_shape: TerminalCursorShape::Block,
            cursor_blink: true,
            copy_on_select: TERMINAL_COPY_ON_SELECT,
            bidi_enabled: TERMINAL_BIDI_ENABLED,
            terminal_encoding: TerminalEncoding::Utf8,
            theme: TerminalUiTheme::default(),
            render_policy: EffectiveRenderPolicy::quality(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct TerminalUiSettings {
    pub(crate) blink_mode: TerminalBlinkMode,
    pub(crate) copy_on_select: bool,
    pub(crate) keep_selection_on_copy: bool,
    pub(crate) bidi_enabled: bool,
}

impl Default for TerminalUiSettings {
    fn default() -> Self {
        Self {
            blink_mode: TERMINAL_BLINK_MODE,
            copy_on_select: TERMINAL_COPY_ON_SELECT,
            keep_selection_on_copy: TERMINAL_KEEP_SELECTION_ON_COPY,
            bidi_enabled: TERMINAL_BIDI_ENABLED,
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
            copy_on_select: preferences.copy_on_select,
            keep_selection_on_copy: TERMINAL_KEEP_SELECTION_ON_COPY,
            bidi_enabled: preferences.bidi_enabled,
        }
    }
}

#[derive(Clone)]
pub struct TerminalUiTheme {
    pub background: u32,
    pub(crate) bell_background: u32,
    pub foreground: u32,
    pub(crate) header_background: u32,
    pub(crate) header_foreground: u32,
}

impl Default for TerminalUiTheme {
    fn default() -> Self {
        Self {
            background: OXIDETERM_TERMINAL_BACKGROUND,
            bell_background: 0x17131a,
            foreground: OXIDETERM_TERMINAL_FOREGROUND,
            header_background: 0x1b1f24,
            header_foreground: 0x8bbdff,
        }
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
            background,
            bell_background: 0x17131a,
            foreground,
            header_background: background,
            header_foreground: cursor,
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
        let font = terminal_font_with_family(&preferences.font_family);
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
    terminal_font_with_family(TERMINAL_FONT)
}

pub(crate) fn terminal_font_with_family(family: &str) -> Font {
    Font {
        family: SharedString::from(family.to_string()),
        features: terminal_font_features(),
        fallbacks: Some(FontFallbacks::from_fonts(vec![
            "JetBrainsMono Nerd Font Mono".to_string(),
            "JetBrains Mono NF (Subset)".to_string(),
            "JetBrains Mono".to_string(),
            "JetBrainsMonoNL Nerd Font Mono".to_string(),
            "MesloLGS Nerd Font Mono".to_string(),
            "Maple Mono NF CN".to_string(),
            "Symbols Nerd Font Mono".to_string(),
            "Symbols Nerd Font".to_string(),
            "ui-monospace".to_string(),
            "SF Mono".to_string(),
            "Menlo".to_string(),
            "Monaco".to_string(),
            "Cascadia Mono".to_string(),
            "DejaVu Sans Mono".to_string(),
            "Noto Sans Mono".to_string(),
            "Liberation Mono".to_string(),
            "Courier New".to_string(),
            "Apple Color Emoji".to_string(),
        ])),
        weight: FontWeight::default(),
        style: FontStyle::Normal,
    }
}

pub(crate) fn terminal_font_features() -> FontFeatures {
    FontFeatures::disable_ligatures()
}

pub(crate) fn terminal_lifecycle_label(lifecycle: &TerminalLifecycle) -> String {
    match lifecycle {
        TerminalLifecycle::Running => "running".to_string(),
        TerminalLifecycle::Exited(Some(code)) => format!("exited({code})"),
        TerminalLifecycle::Exited(None) => "exited".to_string(),
        TerminalLifecycle::Closed => "closed".to_string(),
    }
}

pub(crate) fn terminal_process_header(process_info: &TerminalProcessInfo) -> String {
    let mut parts = Vec::new();
    if let Some(pid) = process_info.shell_pid {
        parts.push(format!("pid {pid}"));
    }
    if let Some(foreground_pid) = process_info
        .foreground_pid
        .filter(|foreground_pid| Some(*foreground_pid) != process_info.shell_pid)
    {
        parts.push(format!("fg {foreground_pid}"));
    }
    if let Some(process_group_id) = process_info
        .foreground_process_group_id
        .filter(|process_group_id| Some(*process_group_id) != process_info.foreground_pid)
    {
        parts.push(format!("pgid {process_group_id}"));
    }
    if let Some(command) = process_info.command.as_deref() {
        parts.push(command.to_string());
    }
    if let Some(cwd) = process_info.cwd.as_ref().and_then(|cwd| cwd.file_name()) {
        parts.push(cwd.to_string_lossy().to_string());
    }

    if parts.is_empty() {
        String::new()
    } else {
        format!(" · {}", parts.join(" · "))
    }
}
