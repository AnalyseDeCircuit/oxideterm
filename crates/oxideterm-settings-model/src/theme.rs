// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Custom theme editor model and serialization helpers.
//!
//! Theme parsing and editor color conversion are settings-domain behavior, not
//! GPUI rendering. The app keeps the modal layout while this module owns the
//! JSON shape, color normalization, and editor value conversion.

use oxideterm_settings::PersistedSettings;
use oxideterm_theme::{
    AppUiColors, BUILT_IN_THEMES, TerminalTheme, ThemeTokens, derive_ui_colors_from_terminal,
    theme_by_id,
};

pub const CUSTOM_THEME_PREFIX: &str = "custom:";
const CUSTOM_THEME_IMPORT_VERSION: u64 = 1;

#[derive(Clone, Copy)]
pub struct ThemeColorField {
    pub json_key: &'static str,
    pub label_key: &'static str,
}

pub const TERMINAL_THEME_COLOR_FIELDS: &[ThemeColorField] = &[
    ThemeColorField {
        json_key: "background",
        label_key: "bg",
    },
    ThemeColorField {
        json_key: "foreground",
        label_key: "fg",
    },
    ThemeColorField {
        json_key: "cursor",
        label_key: "cursor",
    },
    ThemeColorField {
        json_key: "selectionBackground",
        label_key: "selection",
    },
    ThemeColorField {
        json_key: "black",
        label_key: "black",
    },
    ThemeColorField {
        json_key: "red",
        label_key: "red",
    },
    ThemeColorField {
        json_key: "green",
        label_key: "green",
    },
    ThemeColorField {
        json_key: "yellow",
        label_key: "yellow",
    },
    ThemeColorField {
        json_key: "blue",
        label_key: "blue",
    },
    ThemeColorField {
        json_key: "magenta",
        label_key: "magenta",
    },
    ThemeColorField {
        json_key: "cyan",
        label_key: "cyan",
    },
    ThemeColorField {
        json_key: "white",
        label_key: "white",
    },
    ThemeColorField {
        json_key: "brightBlack",
        label_key: "bright_black",
    },
    ThemeColorField {
        json_key: "brightRed",
        label_key: "bright_red",
    },
    ThemeColorField {
        json_key: "brightGreen",
        label_key: "bright_green",
    },
    ThemeColorField {
        json_key: "brightYellow",
        label_key: "bright_yellow",
    },
    ThemeColorField {
        json_key: "brightBlue",
        label_key: "bright_blue",
    },
    ThemeColorField {
        json_key: "brightMagenta",
        label_key: "bright_magenta",
    },
    ThemeColorField {
        json_key: "brightCyan",
        label_key: "bright_cyan",
    },
    ThemeColorField {
        json_key: "brightWhite",
        label_key: "bright_white",
    },
];

pub const UI_THEME_COLOR_FIELDS: &[ThemeColorField] = &[
    ThemeColorField {
        json_key: "bg",
        label_key: "ui_bg",
    },
    ThemeColorField {
        json_key: "bgPanel",
        label_key: "ui_panel",
    },
    ThemeColorField {
        json_key: "bgCard",
        label_key: "ui_bg_card",
    },
    ThemeColorField {
        json_key: "bgHover",
        label_key: "ui_hover",
    },
    ThemeColorField {
        json_key: "bgActive",
        label_key: "ui_active",
    },
    ThemeColorField {
        json_key: "bgSecondary",
        label_key: "ui_bg_secondary",
    },
    ThemeColorField {
        json_key: "bgElevated",
        label_key: "ui_bg_elevated",
    },
    ThemeColorField {
        json_key: "bgSunken",
        label_key: "ui_bg_sunken",
    },
    ThemeColorField {
        json_key: "text",
        label_key: "ui_text",
    },
    ThemeColorField {
        json_key: "textMuted",
        label_key: "ui_text_muted",
    },
    ThemeColorField {
        json_key: "textSecondary",
        label_key: "ui_text_secondary",
    },
    ThemeColorField {
        json_key: "textHeading",
        label_key: "ui_text",
    },
    ThemeColorField {
        json_key: "border",
        label_key: "ui_border",
    },
    ThemeColorField {
        json_key: "borderStrong",
        label_key: "ui_border_strong",
    },
    ThemeColorField {
        json_key: "divider",
        label_key: "ui_divider",
    },
    ThemeColorField {
        json_key: "accent",
        label_key: "ui_accent",
    },
    ThemeColorField {
        json_key: "accentHover",
        label_key: "ui_accent_hover",
    },
    ThemeColorField {
        json_key: "accentText",
        label_key: "ui_accent_text",
    },
    ThemeColorField {
        json_key: "accentSecondary",
        label_key: "ui_accent_secondary",
    },
    ThemeColorField {
        json_key: "success",
        label_key: "ui_success",
    },
    ThemeColorField {
        json_key: "warning",
        label_key: "ui_warning",
    },
    ThemeColorField {
        json_key: "error",
        label_key: "ui_error",
    },
    ThemeColorField {
        json_key: "info",
        label_key: "ui_info",
    },
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeEditorSection {
    Terminal,
    Ui,
}

#[derive(Clone, Debug)]
pub struct ThemeEditorState {
    pub edit_theme_id: Option<String>,
    pub name: String,
    pub duplicate_theme: String,
    pub duplicate_theme_touched: bool,
    pub terminal_colors: Vec<String>,
    pub ui_colors: Vec<String>,
    pub active_section: ThemeEditorSection,
}

pub fn theme_editor_from_settings(
    settings: &PersistedSettings,
    edit_theme_id: Option<String>,
    default_name: String,
) -> ThemeEditorState {
    let fallback_id = valid_builtin_theme_id(&settings.terminal.theme)
        .unwrap_or("azurite")
        .to_string();
    let (name, duplicate_theme, terminal, ui) = edit_theme_id
        .as_deref()
        .and_then(|theme_id| {
            let (terminal, ui) = custom_theme_terminal_and_ui(settings, theme_id)?;
            Some((
                custom_theme_name(settings, theme_id).unwrap_or_else(|| default_name.clone()),
                fallback_id.clone(),
                terminal,
                ui,
            ))
        })
        .unwrap_or_else(|| {
            let duplicate_theme = fallback_id.clone();
            let terminal = theme_by_id(&duplicate_theme).terminal;
            let ui = derive_ui_colors_from_terminal(terminal);
            (default_name, duplicate_theme, terminal, ui)
        });

    ThemeEditorState {
        edit_theme_id,
        name,
        duplicate_theme,
        duplicate_theme_touched: false,
        terminal_colors: terminal_theme_to_colors(terminal),
        ui_colors: app_ui_colors_to_colors(ui),
        active_section: ThemeEditorSection::Terminal,
    }
}

pub fn save_theme_editor_to_settings(
    settings: &mut PersistedSettings,
    editor: ThemeEditorState,
) -> Option<String> {
    let name = editor.name.trim().to_string();
    if name.is_empty() {
        return None;
    }
    let theme_id = editor.edit_theme_id.unwrap_or_else(|| {
        format!(
            "{}{}",
            CUSTOM_THEME_PREFIX,
            slugify_custom_theme_name(&name)
        )
    });
    let terminal = editor_terminal_theme(&editor.terminal_colors);
    let ui = editor_ui_colors(&editor.ui_colors);
    settings
        .custom_themes
        .insert(theme_id.clone(), custom_theme_json(&name, terminal, ui));
    settings.terminal.theme = theme_id;
    Some(name)
}

pub fn delete_custom_theme_from_settings(
    settings: &mut PersistedSettings,
    theme_id: &str,
    fallback_theme_id: &str,
) {
    settings.custom_themes.remove(theme_id);
    if settings.terminal.theme == theme_id {
        settings.terminal.theme = valid_builtin_theme_id(fallback_theme_id)
            .unwrap_or("azurite")
            .to_string();
    }
}

fn valid_builtin_theme_id(id: &str) -> Option<&str> {
    // Keep custom theme editor fallback policy with theme metadata instead of
    // duplicating the same built-in existence check in the GPUI app.
    BUILT_IN_THEMES
        .iter()
        .any(|theme| theme.id == id)
        .then_some(id)
}

pub fn is_custom_theme_id(id: &str) -> bool {
    id.starts_with(CUSTOM_THEME_PREFIX)
}

pub fn custom_theme_display_name(settings: &PersistedSettings, id: &str) -> String {
    custom_theme_name(settings, id).unwrap_or_else(|| theme_display_name(id))
}

pub fn custom_theme_tokens_from_settings(settings: &PersistedSettings) -> Option<ThemeTokens> {
    let (terminal, ui) = custom_theme_terminal_and_ui(settings, &settings.terminal.theme)?;
    let mut tokens = ThemeTokens::from_builtin(theme_by_id("azurite"));
    tokens.terminal = terminal;
    tokens.ui = ui;
    Some(tokens)
}

pub fn custom_theme_terminal_and_ui(
    settings: &PersistedSettings,
    id: &str,
) -> Option<(TerminalTheme, AppUiColors)> {
    if !is_custom_theme_id(id) {
        return None;
    }
    let value = settings.custom_themes.get(id)?;
    let terminal_value = value.get("terminalColors")?;
    let ui_value = value.get("uiColors")?;
    Some((
        terminal_theme_from_value(terminal_value)?,
        app_ui_colors_from_value(ui_value)?,
    ))
}

pub fn terminal_theme_from_value(value: &serde_json::Value) -> Option<TerminalTheme> {
    Some(TerminalTheme {
        background: color_value(value, "background")?,
        foreground: color_value(value, "foreground")?,
        cursor: color_value(value, "cursor")?,
        selection_background: leak_static_hex(color_string_value(value, "selectionBackground")?),
        black: color_value(value, "black")?,
        red: color_value(value, "red")?,
        green: color_value(value, "green")?,
        yellow: color_value(value, "yellow")?,
        blue: color_value(value, "blue")?,
        magenta: color_value(value, "magenta")?,
        cyan: color_value(value, "cyan")?,
        white: color_value(value, "white")?,
        bright_black: color_value(value, "brightBlack")?,
        bright_red: color_value(value, "brightRed")?,
        bright_green: color_value(value, "brightGreen")?,
        bright_yellow: color_value(value, "brightYellow")?,
        bright_blue: color_value(value, "brightBlue")?,
        bright_magenta: color_value(value, "brightMagenta")?,
        bright_cyan: color_value(value, "brightCyan")?,
        bright_white: color_value(value, "brightWhite")?,
    })
}

pub fn app_ui_colors_from_value(value: &serde_json::Value) -> Option<AppUiColors> {
    Some(AppUiColors {
        bg: color_value(value, "bg")?,
        bg_panel: color_value(value, "bgPanel")?,
        bg_card: color_value(value, "bgCard")?,
        bg_hover: color_value(value, "bgHover")?,
        bg_active: color_value(value, "bgActive")?,
        bg_secondary: color_value(value, "bgSecondary")?,
        bg_elevated: color_value(value, "bgElevated")?,
        bg_sunken: color_value(value, "bgSunken")?,
        text: color_value(value, "text")?,
        text_muted: color_value(value, "textMuted")?,
        text_secondary: color_value(value, "textSecondary")?,
        text_heading: color_value(value, "textHeading")
            .or_else(|| color_value(value, "text"))
            .unwrap_or(0xdbeafe),
        border: color_value(value, "border")?,
        border_strong: color_value(value, "borderStrong")?,
        divider: color_value(value, "divider")?,
        accent: color_value(value, "accent")?,
        accent_hover: color_value(value, "accentHover")?,
        accent_text: color_value(value, "accentText")?,
        accent_secondary: color_value(value, "accentSecondary")?,
        success: color_value(value, "success")?,
        warning: color_value(value, "warning")?,
        error: color_value(value, "error")?,
        info: color_value(value, "info")?,
    })
}

pub fn parse_color_hex(value: &str) -> Option<u32> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed.strip_prefix('#') {
        return match hex.len() {
            3 => {
                let mut expanded = String::with_capacity(6);
                for ch in hex.chars() {
                    expanded.push(ch);
                    expanded.push(ch);
                }
                u32::from_str_radix(&expanded, 16).ok()
            }
            6 => u32::from_str_radix(hex, 16).ok(),
            8 => u32::from_str_radix(&hex[..6], 16).ok(),
            _ => None,
        };
    }

    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("rgb(") || lower.starts_with("rgba(") {
        let start = trimmed.find('(')?;
        let end = trimmed.rfind(')')?;
        let mut parts = trimmed[start + 1..end]
            .split(',')
            .map(|part| part.trim().parse::<f32>().ok());
        let red = parts.next()??.round().clamp(0.0, 255.0) as u32;
        let green = parts.next()??.round().clamp(0.0, 255.0) as u32;
        let blue = parts.next()??.round().clamp(0.0, 255.0) as u32;
        return Some((red << 16) | (green << 8) | blue);
    }

    None
}

pub fn format_hex_color(color: u32) -> String {
    format!("#{:06x}", color & 0x00ff_ffff)
}

pub fn terminal_theme_to_colors(theme: TerminalTheme) -> Vec<String> {
    vec![
        format_hex_color(theme.background),
        format_hex_color(theme.foreground),
        format_hex_color(theme.cursor),
        theme.selection_background.to_string(),
        format_hex_color(theme.black),
        format_hex_color(theme.red),
        format_hex_color(theme.green),
        format_hex_color(theme.yellow),
        format_hex_color(theme.blue),
        format_hex_color(theme.magenta),
        format_hex_color(theme.cyan),
        format_hex_color(theme.white),
        format_hex_color(theme.bright_black),
        format_hex_color(theme.bright_red),
        format_hex_color(theme.bright_green),
        format_hex_color(theme.bright_yellow),
        format_hex_color(theme.bright_blue),
        format_hex_color(theme.bright_magenta),
        format_hex_color(theme.bright_cyan),
        format_hex_color(theme.bright_white),
    ]
}

pub fn app_ui_colors_to_colors(ui: AppUiColors) -> Vec<String> {
    vec![
        format_hex_color(ui.bg),
        format_hex_color(ui.bg_panel),
        format_hex_color(ui.bg_card),
        format_hex_color(ui.bg_hover),
        format_hex_color(ui.bg_active),
        format_hex_color(ui.bg_secondary),
        format_hex_color(ui.bg_elevated),
        format_hex_color(ui.bg_sunken),
        format_hex_color(ui.text),
        format_hex_color(ui.text_muted),
        format_hex_color(ui.text_secondary),
        format_hex_color(ui.text_heading),
        format_hex_color(ui.border),
        format_hex_color(ui.border_strong),
        format_hex_color(ui.divider),
        format_hex_color(ui.accent),
        format_hex_color(ui.accent_hover),
        format_hex_color(ui.accent_text),
        format_hex_color(ui.accent_secondary),
        format_hex_color(ui.success),
        format_hex_color(ui.warning),
        format_hex_color(ui.error),
        format_hex_color(ui.info),
    ]
}

pub fn editor_terminal_theme(colors: &[String]) -> TerminalTheme {
    let fallback = theme_by_id("azurite").terminal;
    let color = |index: usize, fallback: u32| {
        colors
            .get(index)
            .and_then(|value| parse_color_hex(value))
            .unwrap_or(fallback)
    };
    TerminalTheme {
        background: color(0, fallback.background),
        foreground: color(1, fallback.foreground),
        cursor: color(2, fallback.cursor),
        selection_background: leak_static_hex(
            colors
                .get(3)
                .and_then(|value| parse_color_hex(value).map(format_hex_color))
                .unwrap_or_else(|| fallback.selection_background.to_string()),
        ),
        black: color(4, fallback.black),
        red: color(5, fallback.red),
        green: color(6, fallback.green),
        yellow: color(7, fallback.yellow),
        blue: color(8, fallback.blue),
        magenta: color(9, fallback.magenta),
        cyan: color(10, fallback.cyan),
        white: color(11, fallback.white),
        bright_black: color(12, fallback.bright_black),
        bright_red: color(13, fallback.bright_red),
        bright_green: color(14, fallback.bright_green),
        bright_yellow: color(15, fallback.bright_yellow),
        bright_blue: color(16, fallback.bright_blue),
        bright_magenta: color(17, fallback.bright_magenta),
        bright_cyan: color(18, fallback.bright_cyan),
        bright_white: color(19, fallback.bright_white),
    }
}

pub fn editor_ui_colors(colors: &[String]) -> AppUiColors {
    let fallback = derive_ui_colors_from_terminal(theme_by_id("azurite").terminal);
    let color = |index: usize, fallback: u32| {
        colors
            .get(index)
            .and_then(|value| parse_color_hex(value))
            .unwrap_or(fallback)
    };
    AppUiColors {
        bg: color(0, fallback.bg),
        bg_panel: color(1, fallback.bg_panel),
        bg_card: color(2, fallback.bg_card),
        bg_hover: color(3, fallback.bg_hover),
        bg_active: color(4, fallback.bg_active),
        bg_secondary: color(5, fallback.bg_secondary),
        bg_elevated: color(6, fallback.bg_elevated),
        bg_sunken: color(7, fallback.bg_sunken),
        text: color(8, fallback.text),
        text_muted: color(9, fallback.text_muted),
        text_secondary: color(10, fallback.text_secondary),
        text_heading: color(11, fallback.text_heading),
        border: color(12, fallback.border),
        border_strong: color(13, fallback.border_strong),
        divider: color(14, fallback.divider),
        accent: color(15, fallback.accent),
        accent_hover: color(16, fallback.accent_hover),
        accent_text: color(17, fallback.accent_text),
        accent_secondary: color(18, fallback.accent_secondary),
        success: color(19, fallback.success),
        warning: color(20, fallback.warning),
        error: color(21, fallback.error),
        info: color(22, fallback.info),
    }
}

pub fn custom_theme_json(
    name: &str,
    terminal: TerminalTheme,
    ui: AppUiColors,
) -> serde_json::Value {
    let terminal_colors = terminal_theme_to_colors(terminal);
    let ui_colors = app_ui_colors_to_colors(ui);
    serde_json::json!({
        "version": CUSTOM_THEME_IMPORT_VERSION,
        "name": name,
        "terminalColors": color_object(TERMINAL_THEME_COLOR_FIELDS, &terminal_colors),
        "uiColors": color_object(UI_THEME_COLOR_FIELDS, &ui_colors),
    })
}

pub fn import_custom_theme(
    json_string: &str,
) -> Result<(String, String, serde_json::Value), String> {
    let mut value = serde_json::from_str::<serde_json::Value>(json_string)
        .map_err(|_| "Invalid JSON".to_string())?;
    let name = value
        .get("name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Invalid theme format".to_string())?
        .to_string();
    let terminal = terminal_theme_from_value(
        value
            .get("terminalColors")
            .ok_or_else(|| "Invalid theme format".to_string())?,
    )
    .ok_or_else(|| "Invalid theme colors".to_string())?;
    let ui = app_ui_colors_from_value(
        value
            .get("uiColors")
            .ok_or_else(|| "Invalid theme format".to_string())?,
    )
    .ok_or_else(|| "Invalid UI colors".to_string())?;
    value = custom_theme_json(&name, terminal, ui);
    let id = format!(
        "{}{}-{}",
        CUSTOM_THEME_PREFIX,
        slugify_import_theme_name(&name),
        current_millis()
    );
    Ok((id, name, value))
}

pub fn slugify_custom_theme_name(name: &str) -> String {
    let mut slug = String::new();
    let mut previous_dash = false;
    for ch in name.to_lowercase().chars() {
        let valid = ch.is_ascii_alphanumeric() || ('\u{4e00}'..='\u{9fff}').contains(&ch);
        if valid {
            slug.push(ch);
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        "untitled".to_string()
    } else {
        slug
    }
}

pub fn custom_theme_name(settings: &PersistedSettings, id: &str) -> Option<String> {
    settings
        .custom_themes
        .get(id)?
        .get("name")?
        .as_str()
        .map(ToOwned::to_owned)
}

fn color_value(value: &serde_json::Value, key: &str) -> Option<u32> {
    parse_color_hex(value.get(key)?.as_str()?)
}

fn color_string_value(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)?
        .as_str()
        .and_then(parse_color_hex)
        .map(format_hex_color)
}

fn leak_static_hex(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn color_object(fields: &[ThemeColorField], colors: &[String]) -> serde_json::Value {
    let mut object = serde_json::Map::new();
    for (index, field) in fields.iter().enumerate() {
        object.insert(
            field.json_key.to_string(),
            serde_json::Value::String(
                colors
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| "#000000".to_string()),
            ),
        );
    }
    serde_json::Value::Object(object)
}

fn slugify_import_theme_name(name: &str) -> String {
    let slug = name
        .to_ascii_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if slug.is_empty() {
        "theme".to_string()
    } else {
        slug
    }
}

fn theme_display_name(id: &str) -> String {
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

fn current_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_hex_accepts_hex_and_rgb_forms() {
        assert_eq!(parse_color_hex("#0f0"), Some(0x00ff00));
        assert_eq!(parse_color_hex("#112233aa"), Some(0x112233));
        assert_eq!(parse_color_hex("rgb(1, 2, 3)"), Some(0x010203));
    }

    #[test]
    fn custom_theme_slug_keeps_cjk_and_collapses_separators() {
        assert_eq!(slugify_custom_theme_name("My 主题!!"), "my-主题");
        assert_eq!(slugify_custom_theme_name("!!!"), "untitled");
    }

    #[test]
    fn saving_theme_editor_owns_custom_theme_persistence() {
        let mut settings = PersistedSettings::default();
        let editor = theme_editor_from_settings(&settings, None, "Mine".to_string());

        let saved_name = save_theme_editor_to_settings(&mut settings, editor);

        assert_eq!(saved_name.as_deref(), Some("Mine"));
        assert!(settings.terminal.theme.starts_with(CUSTOM_THEME_PREFIX));
        assert!(
            settings
                .custom_themes
                .contains_key(&settings.terminal.theme)
        );
    }
}
