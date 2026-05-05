use std::{path::PathBuf, time::Duration};

use gpui::{
    Font, FontFallbacks, FontFeatures, FontStyle, FontWeight, Pixels, SharedString, TextRun,
    Window, px, rgb,
};
use oxideterm_render_policy::EffectiveRenderPolicy;
use oxideterm_terminal::{TerminalColor, TerminalCursorShape, TerminalEncoding};

pub const MAX_HIGHLIGHT_RULES: usize = 32;
pub const MAX_HIGHLIGHT_PATTERN_LENGTH: usize = 512;

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
pub(crate) const TERMINAL_PASTE_PROTECTION: bool = true;
pub(crate) const TERMINAL_SMART_COPY: bool = true;
pub(crate) const TERMINAL_OSC52_CLIPBOARD: bool = true;
pub(crate) const TERMINAL_COPY_ON_SELECT: bool = false;
pub(crate) const TERMINAL_MIDDLE_CLICK_PASTE: bool = false;
pub(crate) const TERMINAL_KEEP_SELECTION_ON_COPY: bool = true;
pub(crate) const TERMINAL_SELECTION_REQUIRES_SHIFT: bool = false;
pub(crate) const TERMINAL_BIDI_ENABLED: bool = true;

#[derive(Clone)]
pub struct TerminalUiPreferences {
    pub font_family: String,
    pub font_size: f32,
    pub line_height: f32,
    pub cursor_shape: TerminalCursorShape,
    pub cursor_blink: bool,
    pub paste_protection: bool,
    pub smart_copy: bool,
    pub osc52_clipboard: bool,
    pub copy_on_select: bool,
    pub middle_click_paste: bool,
    pub selection_requires_shift: bool,
    pub bidi_enabled: bool,
    pub terminal_encoding: TerminalEncoding,
    pub theme: TerminalUiTheme,
    pub render_policy: EffectiveRenderPolicy,
    pub background: Option<TerminalBackgroundPreferences>,
    pub paste_labels: TerminalPasteLabels,
    pub highlight_rules: Vec<TerminalHighlightRule>,
}

impl Default for TerminalUiPreferences {
    fn default() -> Self {
        Self {
            font_family: TERMINAL_FONT.to_string(),
            font_size: TERMINAL_FONT_SIZE,
            line_height: TERMINAL_LINE_HEIGHT_RATIO,
            cursor_shape: TerminalCursorShape::Block,
            cursor_blink: true,
            paste_protection: TERMINAL_PASTE_PROTECTION,
            smart_copy: TERMINAL_SMART_COPY,
            osc52_clipboard: TERMINAL_OSC52_CLIPBOARD,
            copy_on_select: TERMINAL_COPY_ON_SELECT,
            middle_click_paste: TERMINAL_MIDDLE_CLICK_PASTE,
            selection_requires_shift: TERMINAL_SELECTION_REQUIRES_SHIFT,
            bidi_enabled: TERMINAL_BIDI_ENABLED,
            terminal_encoding: TerminalEncoding::Utf8,
            theme: TerminalUiTheme::default(),
            render_policy: EffectiveRenderPolicy::quality(),
            background: None,
            paste_labels: TerminalPasteLabels::default(),
            highlight_rules: Vec::new(),
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
    pub(crate) copy_on_select: bool,
    pub(crate) middle_click_paste: bool,
    pub(crate) keep_selection_on_copy: bool,
    pub(crate) selection_requires_shift: bool,
    pub(crate) bidi_enabled: bool,
}

impl Default for TerminalUiSettings {
    fn default() -> Self {
        Self {
            blink_mode: TERMINAL_BLINK_MODE,
            paste_protection: TERMINAL_PASTE_PROTECTION,
            smart_copy: TERMINAL_SMART_COPY,
            osc52_clipboard: TERMINAL_OSC52_CLIPBOARD,
            copy_on_select: TERMINAL_COPY_ON_SELECT,
            middle_click_paste: TERMINAL_MIDDLE_CLICK_PASTE,
            keep_selection_on_copy: TERMINAL_KEEP_SELECTION_ON_COPY,
            selection_requires_shift: TERMINAL_SELECTION_REQUIRES_SHIFT,
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
            paste_protection: preferences.paste_protection,
            smart_copy: preferences.smart_copy,
            osc52_clipboard: preferences.osc52_clipboard,
            copy_on_select: preferences.copy_on_select,
            middle_click_paste: preferences.middle_click_paste,
            keep_selection_on_copy: TERMINAL_KEEP_SELECTION_ON_COPY,
            selection_requires_shift: preferences.selection_requires_shift,
            bidi_enabled: preferences.bidi_enabled,
        }
    }
}

#[derive(Clone)]
pub struct TerminalUiTheme {
    pub background: u32,
    pub(crate) bell_background: u32,
    pub foreground: u32,
    pub(crate) header_foreground: u32,
}

impl Default for TerminalUiTheme {
    fn default() -> Self {
        Self {
            background: OXIDETERM_TERMINAL_BACKGROUND,
            bell_background: 0x17131a,
            foreground: OXIDETERM_TERMINAL_FOREGROUND,
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
