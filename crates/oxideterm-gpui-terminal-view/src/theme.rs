use gpui::{Hsla, Rgba, rgb};
use oxideterm_settings::TerminalSemanticScheme;
use oxideterm_terminal_emulator::{TerminalColor, TerminalPalette};
use oxideterm_terminal_semantic::{
    CompiledSemanticScheme, SemanticClass, SemanticScheme, SemanticSchemeDocument,
    compile_scheme_document, compiled_builtin_scheme,
};
use oxideterm_theme::{ThemeTokens, default_tokens};
use std::{
    hash::{Hash, Hasher},
    sync::Arc,
};

pub const MAX_HIGHLIGHT_RULES: usize = 32;
pub const MAX_HIGHLIGHT_PATTERN_LENGTH: usize = 512;
const NESTED_SEMANTIC_COLOR_COUNT: u8 = 6;
const TERMINAL_SEMANTIC_ERROR_LINE_BAND_OPACITY: f32 = 0.11;
const TERMINAL_SEMANTIC_WARNING_LINE_BAND_OPACITY: f32 = 0.08;

pub fn resolved_terminal_semantic_scheme(
    built_in: TerminalSemanticScheme,
    custom: Option<&SemanticSchemeDocument>,
) -> Arc<CompiledSemanticScheme> {
    if let Some(custom) = custom
        && let Ok(compiled) = compile_scheme_document(custom)
    {
        return Arc::new(compiled);
    }
    let built_in = match built_in {
        TerminalSemanticScheme::Balanced => SemanticScheme::Balanced,
        TerminalSemanticScheme::Conservative => SemanticScheme::Conservative,
    };
    Arc::new(compiled_builtin_scheme(built_in).clone())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalHighlightRenderMode {
    #[default]
    Background,
    Underline,
    Outline,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TerminalHighlightMatchScope {
    #[default]
    Match,
    LogicalLine,
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
    pub match_scope: TerminalHighlightMatchScope,
    pub preserve_background: bool,
    pub enabled: bool,
    pub priority: i64,
}

#[derive(Clone)]
pub struct TerminalHighlightRuleSetOverride {
    pub id: String,
    pub rules: Arc<[TerminalHighlightRule]>,
}

#[derive(Clone)]
pub struct TerminalUiTheme {
    // Terminal-owned overlays still use app-level UI tokens for Radix-mapped
    // surfaces such as context menus; terminal colors alone are not enough.
    pub tokens: ThemeTokens,
    pub background: u32,
    pub bell_background: u32,
    pub foreground: u32,
    pub header_foreground: u32,
    pub selection_background: Hsla,
}

impl Default for TerminalUiTheme {
    fn default() -> Self {
        Self::from_tokens(default_tokens())
    }
}

pub fn terminal_color_from_hex(hex: u32) -> TerminalColor {
    TerminalColor::rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

/// Resolves a semantic class through the custom scheme before falling back to terminal colors.
pub fn terminal_semantic_color(
    theme: &TerminalUiTheme,
    class: SemanticClass,
    semantic_scheme: &CompiledSemanticScheme,
) -> u32 {
    terminal_semantic_color_from_palette(theme.tokens.terminal, class, semantic_scheme)
}

pub fn terminal_semantic_color_from_palette(
    terminal: oxideterm_theme::TerminalTheme,
    class: SemanticClass,
    semantic_scheme: &CompiledSemanticScheme,
) -> u32 {
    if let Some(color) = semantic_scheme
        .color(class)
        .and_then(|color| u32::from_str_radix(color.strip_prefix('#')?, 16).ok())
    {
        return color;
    }
    match class {
        SemanticClass::Command => terminal.green,
        SemanticClass::Keyword => terminal.bright_magenta,
        SemanticClass::Option => terminal.cyan,
        // Keep punctuation/operators visually neutral so `=` and `--` stay distinct from
        // adjacent yellow string values and cyan option names across both dark and light themes.
        SemanticClass::Operator => {
            blend_terminal_hex(terminal.foreground, terminal.bright_black, 0.45)
        }
        SemanticClass::String => terminal.yellow,
        SemanticClass::Variable => terminal.bright_blue,
        SemanticClass::Link => terminal.bright_cyan,
        SemanticClass::Path => terminal.blue,
        SemanticClass::Address => terminal.bright_green,
        SemanticClass::Weekday => terminal.cyan,
        SemanticClass::Month => terminal.yellow,
        SemanticClass::Timestamp => terminal.green,
        SemanticClass::PermissionRead => terminal.bright_cyan,
        SemanticClass::PermissionWrite => terminal.bright_yellow,
        SemanticClass::PermissionExecute => terminal.bright_green,
        SemanticClass::PermissionSpecial => terminal.bright_magenta,
        SemanticClass::Number => terminal.magenta,
        // Explicitly uncolored: fall back to the terminal's default foreground so
        // columns that should stay plain are not tinted.
        SemanticClass::Plain => terminal.foreground,
        SemanticClass::Comment => terminal.bright_black,
        SemanticClass::Error => terminal.bright_red,
        SemanticClass::Warning => terminal.bright_yellow,
        SemanticClass::Success => terminal.bright_green,
        SemanticClass::Info => terminal.bright_blue,
    }
}

fn blend_terminal_hex(base: u32, toward: u32, ratio: f32) -> u32 {
    let blend = |shift: u32| -> u32 {
        let a = ((base >> shift) & 0xff) as f32;
        let b = ((toward >> shift) & 0xff) as f32;
        (a + (b - a) * ratio).round().clamp(0.0, 255.0) as u32
    };
    (blend(16) << 16) | (blend(8) << 8) | blend(0)
}

/// Resolves presentation variants while keeping the base semantic palette shared by every view.
pub fn terminal_semantic_variant_color(
    theme: &TerminalUiTheme,
    class: SemanticClass,
    style_variant: Option<u8>,
    semantic_scheme: &CompiledSemanticScheme,
) -> u32 {
    terminal_semantic_variant_color_from_palette(
        theme.tokens.terminal,
        class,
        style_variant,
        semantic_scheme,
    )
}

pub fn terminal_semantic_variant_color_from_palette(
    terminal: oxideterm_theme::TerminalTheme,
    class: SemanticClass,
    style_variant: Option<u8>,
    semantic_scheme: &CompiledSemanticScheme,
) -> u32 {
    let Some(depth) = style_variant.filter(|_| class == SemanticClass::Operator) else {
        return terminal_semantic_color_from_palette(terminal, class, semantic_scheme);
    };
    // Six terminal-theme colors keep nested delimiters distinct without creating a second palette.
    match depth % NESTED_SEMANTIC_COLOR_COUNT {
        0 => terminal.yellow,
        1 => terminal.cyan,
        2 => terminal.green,
        3 => terminal.blue,
        4 => terminal.red,
        _ => terminal.magenta,
    }
}

/// Returns the restrained line treatment used for explicit error and warning envelopes.
pub fn terminal_semantic_line_band(
    theme: &TerminalUiTheme,
    class: SemanticClass,
) -> Option<(u32, f32)> {
    terminal_semantic_line_band_from_colors(theme.tokens.ui.error, theme.tokens.ui.warning, class)
}

pub fn terminal_semantic_line_band_from_colors(
    error_color: u32,
    warning_color: u32,
    class: SemanticClass,
) -> Option<(u32, f32)> {
    // Low-opacity bands retain the original terminal background and keep token text dominant.
    match class {
        SemanticClass::Error => Some((error_color, TERMINAL_SEMANTIC_ERROR_LINE_BAND_OPACITY)),
        SemanticClass::Warning => {
            Some((warning_color, TERMINAL_SEMANTIC_WARNING_LINE_BAND_OPACITY))
        }
        _ => None,
    }
}

fn terminal_selection_background(theme: oxideterm_theme::TerminalTheme) -> Hsla {
    // Parse once with the theme, not while laying out each selected cell.
    let value = theme.selection_background.trim();
    let color = Rgba::try_from(value).ok().or_else(|| {
        let lower = value.to_ascii_lowercase();
        let (components, has_alpha) = if let Some(value) = lower.strip_prefix("rgba(") {
            (value.strip_suffix(')')?, true)
        } else {
            (lower.strip_prefix("rgb(")?.strip_suffix(')')?, false)
        };
        let mut parts = components.split(',').map(|part| {
            part.trim()
                .parse::<f32>()
                .ok()
                .filter(|value| value.is_finite())
        });
        let r = parts.next()??.round().clamp(0.0, 255.0) / 255.0;
        let g = parts.next()??.round().clamp(0.0, 255.0) / 255.0;
        let b = parts.next()??.round().clamp(0.0, 255.0) / 255.0;
        let a = if has_alpha {
            parts.next()??.clamp(0.0, 1.0)
        } else {
            1.0
        };
        parts.next().is_none().then_some(Rgba { r, g, b, a })
    });
    color
        .unwrap_or_else(|| rgb(theme.foreground).alpha(0.2))
        .into()
}

impl TerminalUiTheme {
    pub fn new(background: u32, foreground: u32, cursor: u32) -> Self {
        Self {
            background,
            foreground,
            header_foreground: cursor,
            ..Self::default()
        }
    }

    pub fn from_tokens(tokens: ThemeTokens) -> Self {
        Self {
            background: tokens.terminal.background,
            bell_background: 0x17131a,
            foreground: tokens.terminal.foreground,
            header_foreground: tokens.terminal.cursor,
            selection_background: terminal_selection_background(tokens.terminal),
            tokens,
        }
    }

    /// Colors the terminal backend resolves for default, ANSI, and color-query slots, so
    /// emulator output and pane chrome share one theme.
    pub fn palette(&self) -> TerminalPalette {
        TerminalPalette::new(
            terminal_color_from_hex(self.foreground),
            terminal_color_from_hex(self.background),
            terminal_color_from_hex(self.header_foreground),
            terminal_theme_ansi_colors(self.tokens.terminal),
        )
    }

    pub fn visual_colors_eq(&self, other: &Self) -> bool {
        self.background == other.background
            && self.bell_background == other.bell_background
            && self.foreground == other.foreground
            && self.header_foreground == other.header_foreground
            && self.selection_background == other.selection_background
            && self.tokens.ui == other.tokens.ui
            && self.tokens.terminal == other.tokens.terminal
    }

    /// Interpolates visual theme colors without re-parsing selection strings or mutating backend state.
    pub fn interpolate(from: &Self, to: &Self, progress: f32) -> Self {
        let t = progress.clamp(0.0, 1.0);
        let mut tokens = to.tokens;
        tokens.ui = from.tokens.ui.interpolate(to.tokens.ui, t);
        tokens.terminal = from.tokens.terminal.interpolate(to.tokens.terminal, t);
        Self {
            tokens,
            background: lerp_hex_color(from.background, to.background, t),
            bell_background: lerp_hex_color(from.bell_background, to.bell_background, t),
            foreground: lerp_hex_color(from.foreground, to.foreground, t),
            header_foreground: lerp_hex_color(from.header_foreground, to.header_foreground, t),
            selection_background: lerp_hsla_rgba(
                from.selection_background,
                to.selection_background,
                t,
            ),
        }
    }
}

fn terminal_theme_ansi_colors(terminal: oxideterm_theme::TerminalTheme) -> [TerminalColor; 16] {
    [
        terminal.black,
        terminal.red,
        terminal.green,
        terminal.yellow,
        terminal.blue,
        terminal.magenta,
        terminal.cyan,
        terminal.white,
        terminal.bright_black,
        terminal.bright_red,
        terminal.bright_green,
        terminal.bright_yellow,
        terminal.bright_blue,
        terminal.bright_magenta,
        terminal.bright_cyan,
        terminal.bright_white,
    ]
    .map(terminal_color_from_hex)
}

fn dim_terminal_color(color: TerminalColor) -> TerminalColor {
    TerminalColor::rgb(
        (color.r as f32 * 0.7) as u8,
        (color.g as f32 * 0.7) as u8,
        (color.b as f32 * 0.7) as u8,
    )
}

fn lerp_u8_channel(from: u8, to: u8, progress: f32) -> u8 {
    oxideterm_gpui_ui::motion::lerp(from as f32, to as f32, progress)
        .round()
        .clamp(0.0, 255.0) as u8
}

fn lerp_hex_color(from: u32, to: u32, progress: f32) -> u32 {
    let r = lerp_u8_channel(
        ((from >> 16) & 0xff) as u8,
        ((to >> 16) & 0xff) as u8,
        progress,
    );
    let g = lerp_u8_channel(
        ((from >> 8) & 0xff) as u8,
        ((to >> 8) & 0xff) as u8,
        progress,
    );
    let b = lerp_u8_channel((from & 0xff) as u8, (to & 0xff) as u8, progress);
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

fn lerp_terminal_color(from: TerminalColor, to: TerminalColor, progress: f32) -> TerminalColor {
    TerminalColor::rgb(
        lerp_u8_channel(from.r, to.r, progress),
        lerp_u8_channel(from.g, to.g, progress),
        lerp_u8_channel(from.b, to.b, progress),
    )
}

fn lerp_hsla_rgba(from: Hsla, to: Hsla, progress: f32) -> Hsla {
    let from: Rgba = from.into();
    let to: Rgba = to.into();
    Rgba {
        r: oxideterm_gpui_ui::motion::lerp(from.r, to.r, progress),
        g: oxideterm_gpui_ui::motion::lerp(from.g, to.g, progress),
        b: oxideterm_gpui_ui::motion::lerp(from.b, to.b, progress),
        a: oxideterm_gpui_ui::motion::lerp(from.a, to.a, progress),
    }
    .into()
}

pub fn terminal_color_to_hsla(color: TerminalColor) -> Hsla {
    Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: 1.0,
    }
    .into()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum TerminalTransitionColorKey {
    Static,
    TerminalBackground,
    DefaultForeground,
    DefaultDimForeground,
    CursorBackground,
    SelectionBackground,
    PaletteColor(TerminalColor),
}

/// Maps already-resolved target snapshot colors to their mid-transition colors so
/// the terminal view can animate cleanly after a single backend `set_palette`.
#[derive(Clone, Copy)]
pub struct TerminalTransitionColors {
    pub target_background: TerminalColor,
    pub target_background_hsla: Hsla,
    pub target_foreground: TerminalColor,
    pub target_foreground_hsla: Hsla,
    pub target_dim_foreground: TerminalColor,
    pub target_dim_foreground_hsla: Hsla,
    pub target_header_foreground: Hsla,
    pub target_selection_background: Hsla,
    pub interpolated_foreground: Hsla,
    pub interpolated_dim_foreground: Hsla,
    pub interpolated_header_foreground: Hsla,
    pub interpolated_selection_background: Hsla,
    pub ansi_pairs: [(TerminalColor, Hsla); 16],
    pub dim_ansi_pairs: [(TerminalColor, Hsla); 8],
    pub target_terminal: oxideterm_theme::TerminalTheme,
    pub target_ui_error: u32,
    pub target_ui_warning: u32,
    /// Constant across the entire transition so `TerminalLayoutCache` keeps row layouts warm.
    pub layout_signature: u64,
}

impl TerminalTransitionColors {
    pub fn new(
        from: &TerminalUiTheme,
        to: &TerminalUiTheme,
        interpolated: &TerminalUiTheme,
        progress: f32,
    ) -> Self {
        let t = progress.clamp(0.0, 1.0);
        let from_fg = terminal_color_from_hex(from.foreground);
        let to_fg = terminal_color_from_hex(to.foreground);
        let to_bg = terminal_color_from_hex(to.background);
        let to_dim_fg = dim_terminal_color(to_fg);
        let from_ansi = terminal_theme_ansi_colors(from.tokens.terminal);
        let to_ansi = terminal_theme_ansi_colors(to.tokens.terminal);
        let interp_ansi = terminal_theme_ansi_colors(interpolated.tokens.terminal);
        let mut layout_hasher = std::collections::hash_map::DefaultHasher::new();
        from.background.hash(&mut layout_hasher);
        to.background.hash(&mut layout_hasher);
        from.foreground.hash(&mut layout_hasher);
        to.foreground.hash(&mut layout_hasher);
        from.header_foreground.hash(&mut layout_hasher);
        to.header_foreground.hash(&mut layout_hasher);
        to.selection_background.hash(&mut layout_hasher);
        from_ansi.hash(&mut layout_hasher);
        to_ansi.hash(&mut layout_hasher);

        Self {
            target_background: to_bg,
            target_background_hsla: terminal_color_to_hsla(to_bg),
            target_foreground: to_fg,
            target_foreground_hsla: terminal_color_to_hsla(to_fg),
            target_dim_foreground: to_dim_fg,
            target_dim_foreground_hsla: terminal_color_to_hsla(to_dim_fg),
            target_header_foreground: terminal_color_to_hsla(terminal_color_from_hex(
                to.header_foreground,
            )),
            target_selection_background: to.selection_background,
            interpolated_foreground: terminal_color_to_hsla(terminal_color_from_hex(
                interpolated.foreground,
            )),
            interpolated_dim_foreground: terminal_color_to_hsla(lerp_terminal_color(
                dim_terminal_color(from_fg),
                to_dim_fg,
                t,
            )),
            interpolated_header_foreground: terminal_color_to_hsla(terminal_color_from_hex(
                interpolated.header_foreground,
            )),
            interpolated_selection_background: interpolated.selection_background,
            ansi_pairs: std::array::from_fn(|index| {
                (to_ansi[index], terminal_color_to_hsla(interp_ansi[index]))
            }),
            dim_ansi_pairs: std::array::from_fn(|index| {
                let interpolated_dim = lerp_terminal_color(
                    dim_terminal_color(from_ansi[index]),
                    dim_terminal_color(to_ansi[index]),
                    t,
                );
                (
                    dim_terminal_color(to_ansi[index]),
                    terminal_color_to_hsla(interpolated_dim),
                )
            }),
            target_terminal: to.tokens.terminal,
            target_ui_error: to.tokens.ui.error,
            target_ui_warning: to.tokens.ui.warning,
            layout_signature: layout_hasher.finish(),
        }
    }

    pub fn map_ansi_color(&self, color: TerminalColor) -> Option<Hsla> {
        self.ansi_pairs
            .iter()
            .chain(&self.dim_ansi_pairs)
            .find_map(|&(target, interpolated)| (target == color).then_some(interpolated))
    }

    pub fn resolve_row_color(
        &self,
        key: TerminalTransitionColorKey,
        static_color: Hsla,
        live_terminal_background: Hsla,
    ) -> Hsla {
        match key {
            TerminalTransitionColorKey::Static => static_color,
            TerminalTransitionColorKey::TerminalBackground => live_terminal_background,
            TerminalTransitionColorKey::DefaultForeground => self.interpolated_foreground,
            TerminalTransitionColorKey::DefaultDimForeground => self.interpolated_dim_foreground,
            TerminalTransitionColorKey::CursorBackground => self.interpolated_header_foreground,
            TerminalTransitionColorKey::SelectionBackground => {
                self.interpolated_selection_background
            }
            TerminalTransitionColorKey::PaletteColor(color) => {
                self.map_ansi_color(color).unwrap_or(static_color)
            }
        }
    }
}
