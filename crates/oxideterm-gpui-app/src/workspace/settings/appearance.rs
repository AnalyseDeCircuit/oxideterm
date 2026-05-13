const CUSTOM_THEME_PREFIX: &str = "custom:";
const CUSTOM_THEME_IMPORT_VERSION: u64 = 1;
const THEME_EDITOR_MODAL_WIDTH: f32 = 672.0; // Tauri ThemeEditorModal max-w-2xl.
const THEME_EDITOR_MODAL_MAX_HEIGHT: f32 = 760.0; // Tauri max-h-[85vh] on the default native window.
const THEME_EDITOR_HEADER_PADDING_X: f32 = 16.0; // DialogHeader px-4.
const THEME_EDITOR_HEADER_PADDING_Y: f32 = 12.0; // DialogHeader py-3.
const THEME_EDITOR_BODY_PADDING_X: f32 = 16.0; // Body px-4.
const THEME_EDITOR_BODY_PADDING_Y: f32 = 12.0; // Body py-3.
const THEME_EDITOR_BODY_GAP: f32 = 16.0; // Tauri space-y-4.
const THEME_EDITOR_INPUT_HEIGHT: f32 = 32.0; // Tauri Input h-8.
const THEME_EDITOR_DUPLICATE_WIDTH: f32 = 180.0; // Tauri duplicate select w-[180px].
const THEME_EDITOR_FIELD_SWATCH_SIZE: f32 = 28.0; // Tauri ColorSwatch w-7 h-7.
const THEME_EDITOR_HEX_INPUT_WIDTH: f32 = 72.0; // Tauri inline hex input w-[72px].
const THEME_EDITOR_GRID_COLUMNS: usize = 4; // Tauri grid-cols-4.
const THEME_EDITOR_CHROME_DOT_SIZE: f32 = 10.0; // Tauri preview dots w-2.5 h-2.5.
const THEME_EDITOR_STATUS_DOT_SIZE: f32 = 8.0; // Tauri preview status dots w-2 h-2.
const THEME_EDITOR_PREVIEW_CURSOR_WIDTH: f32 = 8.0; // Tauri cursor w-2.
const THEME_EDITOR_PREVIEW_CURSOR_HEIGHT: f32 = 16.0; // Tauri cursor h-4.
const THEME_EDITOR_SWATCH_LABEL_SIZE: f32 = 10.0; // Tauri text-[10px].
const THEME_EDITOR_SECTION_TITLE_SIZE: f32 = 11.0; // Tauri section heading text-[11px].
const BACKGROUND_THUMBNAIL_ASPECT_RATIO: f32 = 16.0 / 9.0; // Tauri aspect-video.

#[derive(Clone, Copy)]
struct ThemeColorField {
    json_key: &'static str,
    label_key: &'static str,
}

const TERMINAL_THEME_COLOR_FIELDS: &[ThemeColorField] = &[
    ThemeColorField { json_key: "background", label_key: "bg" },
    ThemeColorField { json_key: "foreground", label_key: "fg" },
    ThemeColorField { json_key: "cursor", label_key: "cursor" },
    ThemeColorField { json_key: "selectionBackground", label_key: "selection" },
    ThemeColorField { json_key: "black", label_key: "black" },
    ThemeColorField { json_key: "red", label_key: "red" },
    ThemeColorField { json_key: "green", label_key: "green" },
    ThemeColorField { json_key: "yellow", label_key: "yellow" },
    ThemeColorField { json_key: "blue", label_key: "blue" },
    ThemeColorField { json_key: "magenta", label_key: "magenta" },
    ThemeColorField { json_key: "cyan", label_key: "cyan" },
    ThemeColorField { json_key: "white", label_key: "white" },
    ThemeColorField { json_key: "brightBlack", label_key: "bright_black" },
    ThemeColorField { json_key: "brightRed", label_key: "bright_red" },
    ThemeColorField { json_key: "brightGreen", label_key: "bright_green" },
    ThemeColorField { json_key: "brightYellow", label_key: "bright_yellow" },
    ThemeColorField { json_key: "brightBlue", label_key: "bright_blue" },
    ThemeColorField { json_key: "brightMagenta", label_key: "bright_magenta" },
    ThemeColorField { json_key: "brightCyan", label_key: "bright_cyan" },
    ThemeColorField { json_key: "brightWhite", label_key: "bright_white" },
];

const UI_THEME_COLOR_FIELDS: &[ThemeColorField] = &[
    ThemeColorField { json_key: "bg", label_key: "ui_bg" },
    ThemeColorField { json_key: "bgPanel", label_key: "ui_panel" },
    ThemeColorField { json_key: "bgCard", label_key: "ui_bg_card" },
    ThemeColorField { json_key: "bgHover", label_key: "ui_hover" },
    ThemeColorField { json_key: "bgActive", label_key: "ui_active" },
    ThemeColorField { json_key: "bgSecondary", label_key: "ui_bg_secondary" },
    ThemeColorField { json_key: "bgElevated", label_key: "ui_bg_elevated" },
    ThemeColorField { json_key: "bgSunken", label_key: "ui_bg_sunken" },
    ThemeColorField { json_key: "text", label_key: "ui_text" },
    ThemeColorField { json_key: "textMuted", label_key: "ui_text_muted" },
    ThemeColorField { json_key: "textSecondary", label_key: "ui_text_secondary" },
    ThemeColorField { json_key: "textHeading", label_key: "ui_text" },
    ThemeColorField { json_key: "border", label_key: "ui_border" },
    ThemeColorField { json_key: "borderStrong", label_key: "ui_border_strong" },
    ThemeColorField { json_key: "divider", label_key: "ui_divider" },
    ThemeColorField { json_key: "accent", label_key: "ui_accent" },
    ThemeColorField { json_key: "accentHover", label_key: "ui_accent_hover" },
    ThemeColorField { json_key: "accentText", label_key: "ui_accent_text" },
    ThemeColorField { json_key: "accentSecondary", label_key: "ui_accent_secondary" },
    ThemeColorField { json_key: "success", label_key: "ui_success" },
    ThemeColorField { json_key: "warning", label_key: "ui_warning" },
    ThemeColorField { json_key: "error", label_key: "ui_error" },
    ThemeColorField { json_key: "info", label_key: "ui_info" },
];

pub(super) fn is_custom_theme_id(id: &str) -> bool {
    id.starts_with(CUSTOM_THEME_PREFIX)
}

pub(super) fn custom_theme_display_name(settings: &PersistedSettings, id: &str) -> String {
    custom_theme_name(settings, id).unwrap_or_else(|| theme_display_name(id))
}

pub(super) fn custom_theme_tokens_from_settings(
    settings: &PersistedSettings,
) -> Option<ThemeTokens> {
    let (terminal, ui) = custom_theme_terminal_and_ui(settings, &settings.terminal.theme)?;
    let mut tokens = ThemeTokens::from_builtin(theme_by_id("azurite"));
    tokens.terminal = terminal;
    tokens.ui = ui;
    Some(tokens)
}

fn custom_theme_name(settings: &PersistedSettings, id: &str) -> Option<String> {
    settings
        .custom_themes
        .get(id)?
        .get("name")?
        .as_str()
        .map(ToOwned::to_owned)
}

fn custom_theme_terminal_and_ui(
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

fn terminal_theme_from_value(value: &serde_json::Value) -> Option<TerminalTheme> {
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

fn app_ui_colors_from_value(value: &serde_json::Value) -> Option<AppUiColors> {
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

fn parse_color_hex(value: &str) -> Option<u32> {
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

fn format_hex_color(color: u32) -> String {
    format!("#{:06x}", color & 0x00ff_ffff)
}

fn leak_static_hex(value: String) -> &'static str {
    Box::leak(value.into_boxed_str())
}

fn terminal_theme_to_colors(theme: TerminalTheme) -> Vec<String> {
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

fn app_ui_colors_to_colors(ui: AppUiColors) -> Vec<String> {
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

fn editor_terminal_theme(colors: &[String]) -> TerminalTheme {
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

fn editor_ui_colors(colors: &[String]) -> AppUiColors {
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

fn custom_theme_json(name: &str, terminal: TerminalTheme, ui: AppUiColors) -> serde_json::Value {
    let terminal_colors = terminal_theme_to_colors(terminal);
    let ui_colors = app_ui_colors_to_colors(ui);
    serde_json::json!({
        "version": CUSTOM_THEME_IMPORT_VERSION,
        "name": name,
        "terminalColors": color_object(TERMINAL_THEME_COLOR_FIELDS, &terminal_colors),
        "uiColors": color_object(UI_THEME_COLOR_FIELDS, &ui_colors),
    })
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

fn import_custom_theme(json_string: &str) -> Result<(String, String, serde_json::Value), String> {
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

fn slugify_custom_theme_name(name: &str) -> String {
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

fn current_millis() -> u128 {
    SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

impl WorkspaceApp {
    fn settings_appearance(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let settings = self.settings_store.settings();
        vec![
            self.appearance_theme_card(settings, cx),
            self.appearance_layout_card(settings, cx),
            self.appearance_background_card(settings, cx),
        ]
    }

    fn appearance_theme_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card(
            self.i18n.t("settings_view.appearance.theme"),
            Some(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.appearance_action_button(
                        LucideIcon::Upload,
                        self.i18n.t("settings_view.appearance.theme_import"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.import_theme_from_file(cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .when(is_custom_theme_id(&settings.terminal.theme), |actions| {
                        actions.child(
                            self.appearance_action_button(
                                LucideIcon::Pencil,
                                self.i18n.t("settings_view.custom_theme.edit"),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, _window, cx| {
                                    let theme_id =
                                        this.settings_store.settings().terminal.theme.clone();
                                    this.open_theme_editor(Some(theme_id), cx);
                                    cx.stop_propagation();
                                }),
                            ),
                        )
                    })
                    .child(self.appearance_action_button(
                        LucideIcon::Plus,
                        self.i18n.t("settings_view.custom_theme.create"),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.open_theme_editor(None, cx);
                            cx.stop_propagation();
                        }),
                    ))
                    .into_any_element(),
            ),
            vec![
                self.appearance_row(
                    "settings_view.appearance.color_theme",
                    "settings_view.appearance.color_theme_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceTheme,
                        custom_theme_display_name(settings, &settings.terminal.theme),
                        self.tokens.metrics.settings_select_width,
                        cx,
                    ),
                ),
                self.appearance_theme_preview(settings),
            ],
        )
    }

    fn appearance_layout_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_card(
            self.i18n.t("settings_view.appearance.layout"),
            None,
            vec![
                self.appearance_row(
                    "settings_view.appearance.density",
                    "settings_view.appearance.density_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceDensity,
                        density_label(settings.appearance.ui_density, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.border_radius",
                    "settings_view.appearance.border_radius_hint",
                    self.appearance_radius_control(settings, cx),
                ),
                self.appearance_row(
                    "settings_view.appearance.ui_font",
                    "settings_view.appearance.ui_font_hint",
                    self.appearance_text_input_control(
                        SettingsInput::AppearanceUiFont,
                        settings.appearance.ui_font_family.clone(),
                        self.i18n.t("settings_view.appearance.ui_font_placeholder"),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.animation",
                    "settings_view.appearance.animation_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceAnimation,
                        animation_label(settings.appearance.animation_speed, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.render_profile",
                    "settings_view.appearance.render_profile_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceRenderProfile,
                        render_profile_label(settings.appearance.render_profile, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.appearance.frosted_glass",
                    "settings_view.appearance.frosted_glass_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceFrostedGlass,
                        frosted_glass_label(settings.appearance.frosted_glass, &self.i18n),
                        self.tokens.metrics.settings_appearance_select_width,
                        cx,
                    ),
                ),
            ],
        )
    }

    fn appearance_background_card(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let background_blur = self
            .background_blur_preview
            .unwrap_or(settings.terminal.background_blur);
        self.appearance_card_with_icon(
            LucideIcon::Image,
            self.i18n.t("settings_view.terminal.bg_title"),
            vec![
                self.appearance_checkbox_row(
                    "settings_view.terminal.bg_enabled",
                    "settings_view.terminal.bg_enabled_hint",
                    settings.terminal.background_enabled,
                    set_terminal_background_enabled,
                    cx,
                ),
                self.appearance_background_gallery(settings, cx),
                self.appearance_row(
                    "settings_view.terminal.bg_opacity",
                    "settings_view.terminal.bg_opacity_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceBackgroundOpacity,
                        SelectAnchorId::SettingsAppearanceBackgroundOpacitySlider,
                        3.0,
                        50.0,
                        (settings.terminal.background_opacity * 100.0).round() as f32,
                        "%",
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.terminal.bg_blur",
                    "settings_view.terminal.bg_blur_hint",
                    self.appearance_slider_value_control(
                        SettingsSlider::AppearanceBackgroundBlur,
                        SelectAnchorId::SettingsAppearanceBackgroundBlurSlider,
                        0.0,
                        20.0,
                        background_blur as f32,
                        "px",
                        cx,
                    ),
                ),
                self.appearance_row(
                    "settings_view.terminal.bg_fit",
                    "settings_view.terminal.bg_fit_hint",
                    self.appearance_select_control(
                        SettingsSelect::AppearanceBackgroundFit,
                        background_fit_label(settings.terminal.background_fit, &self.i18n),
                        self.tokens.metrics.settings_appearance_fit_select_width,
                        cx,
                    ),
                ),
                self.appearance_background_tabs(settings, cx),
            ],
        )
    }

    fn appearance_card(
        &self,
        title: String,
        actions: Option<AnyElement>,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(
            div()
                .w_full()
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(self.appearance_card_title(title, None))
                .when_some(actions, |header, actions| header.child(actions))
                .into_any_element(),
            rows,
        )
    }

    fn appearance_card_with_icon(
        &self,
        icon: LucideIcon,
        title: String,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        self.appearance_card_shell(self.appearance_card_title(title, Some(icon)), rows)
    }

    fn appearance_card_shell(&self, header: AnyElement, rows: Vec<AnyElement>) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(self.settings_panel_background(self.tokens.ui.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_card_gap))
            .child(header)
            .children(rows)
            .into_any_element()
    }

    fn appearance_card_title(&self, title: String, icon: Option<LucideIcon>) -> AnyElement {
        let mut title_el = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text));
        if let Some(icon) = icon {
            title_el = title_el.child(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(self.tokens.ui.text),
            ));
        }
        title_el.child(title.to_uppercase()).into_any_element()
    }

    fn appearance_action_button(&self, icon: LucideIcon, label: String) -> Div {
        div()
            .h(px(self.tokens.metrics.settings_appearance_action_height))
            .px(px(10.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgba(0x00000000))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
            .child(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            ))
            .child(label)
    }

    fn appearance_row(&self, label_key: &str, hint_key: &str, control: AnyElement) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap(px(self.tokens.metrics.settings_row_gap))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t(hint_key)),
                    ),
            )
            .child(control)
            .into_any_element()
    }

    fn appearance_checkbox_row(
        &self,
        label_key: &str,
        hint_key: &str,
        checked: bool,
        setter: fn(&mut PersistedSettings, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.appearance_row(
            label_key,
            hint_key,
            checkbox(&self.tokens, String::new(), checked)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.edit_settings(|settings| setter(settings, !checked), cx);
                    }),
                )
                .into_any_element(),
        )
    }

    fn appearance_select_control(
        &self,
        select_id: SettingsSelect,
        value: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let anchor_id = select_id.anchor_id();
        let workspace = cx.entity();
        let trigger = select_trigger(&self.tokens, value, false, false)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.focused_settings_input = None;
                    this.open_settings_select = if this.open_settings_select == Some(select_id) {
                        None
                    } else {
                        Some(select_id)
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );
        div()
            .relative()
            .w(px(width))
            .min_w(px(0.0))
            .child(select_anchor_probe(
                anchor_id,
                trigger,
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn appearance_text_input_control(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: display_value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(width))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn appearance_radius_control(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .size(px(28.0))
                    .rounded(px(settings.appearance.border_radius as f32))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_secondary)),
            )
            .child(self.appearance_slider_control(
                SettingsSlider::AppearanceBorderRadius,
                SelectAnchorId::SettingsAppearanceBorderRadiusSlider,
                0.0,
                24.0,
                settings.appearance.border_radius as f32,
                cx,
            ))
            .child(
                div()
                    .w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{}px", settings.appearance.border_radius)),
            )
            .into_any_element()
    }

    fn appearance_slider_value_control(
        &self,
        slider: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        unit: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(12.0))
            .child(self.appearance_slider_control(slider, anchor_id, min, max, value, cx))
            .child(
                div()
                    .w(px(48.0))
                    .text_align(gpui::TextAlign::Right)
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(format!("{}{}", value.round() as i64, unit)),
            )
            .into_any_element()
    }

    fn appearance_slider_control(
        &self,
        slider_id: SettingsSlider,
        anchor_id: SelectAnchorId,
        min: f32,
        max: f32,
        value: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        div()
            .w(px(self.tokens.metrics.settings_slider_width))
            .child(select_anchor_probe(
                anchor_id,
                slider(
                    &self.tokens,
                    SliderView {
                        min,
                        max,
                        value,
                        disabled: false,
                    },
                )
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        this.open_settings_select = None;
                        this.focused_settings_input = None;
                        this.settings_slider_drag = Some(slider_id);
                        this.apply_settings_slider_from_position(
                            slider_id,
                            f32::from(event.position.x),
                            cx,
                        );
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        this.finish_settings_slider_drag(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &MouseMoveEvent, _window, cx| {
                        this.update_settings_slider_drag(event, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn appearance_theme_preview(&self, settings: &PersistedSettings) -> AnyElement {
        let terminal = self.tokens.terminal;
        div()
            .w_full()
            .mt(px(self.tokens.metrics.settings_font_preview_margin_top))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(terminal.background))
            .p(px(self.tokens.metrics.settings_theme_preview_padding))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(self.tokens.metrics.settings_theme_preview_dot_gap))
                    .child(self.preview_dot(terminal.red))
                    .child(self.preview_dot(terminal.yellow))
                    .child(self.preview_dot(terminal.green)),
            )
            .child(
                div()
                    .font_family(
                        settings
                            .terminal
                            .font_family
                            .terminal_family_name(&settings.terminal.custom_font_family),
                    )
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(self.tokens.metrics.settings_theme_preview_line_height))
                    .text_color(rgb(terminal.foreground))
                    .flex()
                    .flex_col()
                    .child("$ echo \"Hello World\"")
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .gap(px(6.0))
                            .child(div().text_color(rgb(terminal.blue)).child("~"))
                            .child(div().text_color(rgb(terminal.magenta)).child("git"))
                            .child(div().text_color(rgb(terminal.blue)).child("status")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(4.0))
                            .child(">")
                            .child(div().w(px(9.0)).h(px(18.0)).bg(rgb(terminal.cursor))),
                    ),
            )
            .into_any_element()
    }

    fn preview_dot(&self, color: u32) -> AnyElement {
        div()
            .size(px(self.tokens.metrics.settings_theme_preview_dot_size))
            .rounded_full()
            .bg(rgb(color))
            .into_any_element()
    }

    fn render_theme_editor_modal(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let editor = self.theme_editor.as_ref()?;
        let terminal = editor_terminal_theme(&editor.terminal_colors);
        let ui = editor_ui_colors(&editor.ui_colors);
        let title_key = if editor.edit_theme_id.is_some() {
            "settings_view.custom_theme.edit_title"
        } else {
            "settings_view.custom_theme.create_title"
        };
        let save_disabled = editor.name.trim().is_empty();

        let dialog = div()
            .w(px(THEME_EDITOR_MODAL_WIDTH))
            .max_h(px(THEME_EDITOR_MODAL_MAX_HEIGHT))
            .rounded(px(self.tokens.radii.md))
            .overflow_hidden()
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(self.tokens.ui.bg_elevated))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex_none()
                    .px(px(THEME_EDITOR_HEADER_PADDING_X))
                    .py(px(THEME_EDITOR_HEADER_PADDING_Y))
                    .border_b_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.custom_theme.description")),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scrollbar()
                    .px(px(THEME_EDITOR_BODY_PADDING_X))
                    .py(px(THEME_EDITOR_BODY_PADDING_Y))
                    .flex()
                    .flex_col()
                    .gap(px(THEME_EDITOR_BODY_GAP))
                    .child(self.theme_editor_name_duplicate_row(editor, cx))
                    .child(self.theme_editor_preview(editor, terminal, ui))
                    .child(self.theme_editor_section_tabs(editor, cx))
                    .child(self.theme_editor_color_grid(editor, cx)),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(THEME_EDITOR_HEADER_PADDING_X))
                    .py(px(THEME_EDITOR_HEADER_PADDING_Y))
                    .border_t_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .child(if editor.edit_theme_id.is_some() {
                        self.theme_editor_footer_button(
                            LucideIcon::Trash2,
                            self.i18n.t("settings_view.custom_theme.delete"),
                            self.tokens.ui.error,
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                this.delete_theme_editor_theme(cx);
                                cx.stop_propagation();
                            }),
                        )
                        .into_any_element()
                    } else {
                        div().into_any_element()
                    })
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("settings_view.custom_theme.cancel"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Outline,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.close_theme_editor(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                            .child(
                                button_with(
                                    &self.tokens,
                                    self.i18n.t("settings_view.custom_theme.save"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Default,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Md,
                                        disabled: save_disabled,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        if !save_disabled {
                                            this.save_theme_editor(cx);
                                        }
                                        cx.stop_propagation();
                                    }),
                                ),
                            ),
                    ),
            );

        Some(dialog_backdrop().child(dialog).into_any_element())
    }

    fn theme_editor_name_duplicate_row(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .items_end()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(self.theme_editor_label("settings_view.custom_theme.name"))
                    .child(self.theme_editor_text_input(
                        SettingsInput::CustomThemeName,
                        editor.name.clone(),
                        self.i18n.t("settings_view.custom_theme.name_placeholder"),
                        0.0,
                        false,
                        true,
                        cx,
                    )),
            )
            .when(editor.edit_theme_id.is_none(), |row| {
                row.child(self.theme_editor_duplicate_row(editor, cx))
            })
            .into_any_element()
    }

    fn theme_editor_duplicate_row(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = if editor.duplicate_theme_touched {
            theme_display_name(&editor.duplicate_theme)
        } else {
            self.i18n.t("settings_view.custom_theme.select_base")
        };
        div()
            .w(px(THEME_EDITOR_DUPLICATE_WIDTH))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(self.theme_editor_label("settings_view.custom_theme.duplicate_from"))
            .child(self.theme_editor_duplicate_select(value, cx))
            .into_any_element()
    }

    fn theme_editor_duplicate_select(
        &self,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select_id = SettingsSelect::CustomThemeDuplicate;
        let workspace = cx.entity();
        div()
            .relative()
            .w(px(THEME_EDITOR_DUPLICATE_WIDTH))
            .child(select_anchor_probe(
                select_id.anchor_id(),
                select_trigger(&self.tokens, value, false, false)
                    .h(px(THEME_EDITOR_INPUT_HEIGHT))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.focused_settings_input = None;
                            this.open_settings_select =
                                if this.open_settings_select == Some(select_id) {
                                    None
                                } else {
                                    Some(select_id)
                                };
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_select_anchor(anchor, cx);
                    });
                },
            ))
            .into_any_element()
    }

    fn theme_editor_preview(
        &self,
        editor: &ThemeEditorState,
        terminal: TerminalTheme,
        ui: AppUiColors,
    ) -> AnyElement {
        div()
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgb(self.tokens.ui.border))
            .bg(rgb(terminal.background))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .bg(rgb(ui.bg_panel))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.theme_editor_preview_dot(
                        terminal.red,
                        THEME_EDITOR_CHROME_DOT_SIZE,
                    ))
                    .child(self.theme_editor_preview_dot(
                        terminal.yellow,
                        THEME_EDITOR_CHROME_DOT_SIZE,
                    ))
                    .child(self.theme_editor_preview_dot(
                        terminal.green,
                        THEME_EDITOR_CHROME_DOT_SIZE,
                    ))
                    .child(
                        div()
                            .ml(px(8.0))
                            .text_size(px(THEME_EDITOR_SWATCH_LABEL_SIZE))
                            .text_color(rgb(ui.text_muted))
                            .child(format!("Terminal - {}", editor.name)),
                    ),
            )
            .child(
                div()
                    .p(px(12.0))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(20.0))
                    .text_color(rgb(terminal.foreground))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .flex()
                            .child(div().text_color(rgb(terminal.green)).child("user@oxide"))
                            .child(":")
                            .child(div().text_color(rgb(terminal.blue)).child("~/projects"))
                            .child("$ ")
                            .child(div().text_color(rgb(terminal.magenta)).child("git"))
                            .child(" status"),
                    )
                    .child(div().text_color(rgb(terminal.yellow)).child("On branch main"))
                    .child(
                        div()
                            .text_color(rgb(terminal.cyan))
                            .child("Changes not staged for commit:"),
                    )
                    .child(
                        div()
                            .flex()
                            .child(div().text_color(rgb(terminal.red)).child("  modified: "))
                            .child("src/main.rs"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .child(div().text_color(rgb(terminal.green)).child("user@oxide"))
                            .child(":")
                            .child(div().text_color(rgb(terminal.blue)).child("~"))
                            .child("$ ")
                            .child(
                                div()
                                    .w(px(THEME_EDITOR_PREVIEW_CURSOR_WIDTH))
                                    .h(px(THEME_EDITOR_PREVIEW_CURSOR_HEIGHT))
                                    .bg(rgb(terminal.cursor)),
                            ),
                    ),
            )
            .child(
                div()
                    .px(px(12.0))
                    .py(px(6.0))
                    .border_t_1()
                    .border_color(rgb(ui.border))
                    .bg(rgb(ui.bg))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.theme_editor_preview_badge(
                        "Active",
                        ui.accent,
                        ui.accent_text,
                        false,
                    ))
                    .child(self.theme_editor_preview_badge(
                        "Hover",
                        ui.bg_hover,
                        ui.text_muted,
                        false,
                    ))
                    .child(self.theme_editor_preview_badge("Panel", ui.bg_panel, ui.text, true))
                    .child(
                        div()
                            .ml_auto()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(self.theme_editor_preview_dot(
                                ui.success,
                                THEME_EDITOR_STATUS_DOT_SIZE,
                            ))
                            .child(self.theme_editor_preview_dot(
                                ui.warning,
                                THEME_EDITOR_STATUS_DOT_SIZE,
                            ))
                            .child(self.theme_editor_preview_dot(
                                ui.error,
                                THEME_EDITOR_STATUS_DOT_SIZE,
                            ))
                            .child(self.theme_editor_preview_dot(
                                ui.info,
                                THEME_EDITOR_STATUS_DOT_SIZE,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn theme_editor_preview_dot(&self, color: u32, size: f32) -> AnyElement {
        div()
            .size(px(size))
            .rounded_full()
            .bg(rgb(color))
            .into_any_element()
    }

    fn theme_editor_preview_badge(
        &self,
        label: &'static str,
        background: u32,
        text: u32,
        bordered: bool,
    ) -> AnyElement {
        div()
            .px(px(6.0))
            .py(px(2.0))
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(9.0))
            .text_color(rgb(text))
            .bg(rgb(background))
            .when(bordered, |badge| {
                badge.border_1().border_color(rgb(self.tokens.ui.border))
            })
            .child(label)
            .into_any_element()
    }

    fn theme_editor_section_tabs(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_row()
            .border_b_1()
            .border_color(rgb(self.tokens.ui.border))
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Terminal,
                "settings_view.custom_theme.terminal_colors",
                editor.active_section,
                cx,
            ))
            .child(self.theme_editor_section_tab(
                ThemeEditorSection::Ui,
                "settings_view.custom_theme.ui_colors",
                editor.active_section,
                cx,
            ))
            .into_any_element()
    }

    fn theme_editor_section_tab(
        &self,
        section: ThemeEditorSection,
        label_key: &str,
        active_section: ThemeEditorSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = section == active_section;
        div()
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .bg(rgba(0x00000000))
            .cursor_pointer()
            .hover(|tab| tab.text_color(rgb(self.tokens.ui.text)))
            .child(div().child(self.i18n.t(label_key)))
            .child(
                div()
                    .h(px(2.0))
                    .w_full()
                    .bg(if active {
                        rgb(self.tokens.ui.accent)
                    } else {
                        rgba(0x00000000)
                    }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if let Some(editor) = this.theme_editor.as_mut() {
                        editor.active_section = section;
                    }
                    this.open_settings_select = None;
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn theme_editor_color_grid(
        &self,
        editor: &ThemeEditorState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if editor.active_section == ThemeEditorSection::Ui {
            return self.theme_editor_ui_color_sections(cx);
        }

        let (fields, colors, section) = match editor.active_section {
            ThemeEditorSection::Terminal => (
                TERMINAL_THEME_COLOR_FIELDS,
                editor.terminal_colors.as_slice(),
                ThemeEditorSection::Terminal,
            ),
            ThemeEditorSection::Ui => unreachable!("UI colors render grouped sections"),
        };
        self.theme_editor_color_grid_for_fields(fields, colors, section, cx)
    }

    fn theme_editor_ui_color_sections(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(editor) = self.theme_editor.as_ref() else {
            return div().into_any_element();
        };
        let colors = editor.ui_colors.as_slice();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.custom_theme.ui_colors_hint")),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("settings_view.custom_theme.auto_derive"),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Md,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _event, _window, cx| {
                                if let Some(editor) = this.theme_editor.as_mut() {
                                    let ui = derive_ui_colors_from_terminal(editor_terminal_theme(
                                        &editor.terminal_colors,
                                    ));
                                    editor.ui_colors = app_ui_colors_to_colors(ui);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    ),
            )
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_background",
                &[0, 1, 2, 3, 4, 5, 6, 7],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_text",
                &[8, 9, 10],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_border",
                &[12, 13, 14],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_accent",
                &[15, 16, 17, 18],
                colors,
                cx,
            ))
            .child(self.theme_editor_ui_section(
                "settings_view.custom_theme.section_semantic",
                &[19, 20, 21, 22],
                colors,
                cx,
            ))
            .into_any_element()
    }

    fn theme_editor_ui_section(
        &self,
        title_key: &str,
        indexes: &[usize],
        colors: &[String],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut grid = div()
            .w_full()
            .grid()
            .grid_cols(THEME_EDITOR_GRID_COLUMNS as u16)
            .gap_x(px(16.0))
            .gap_y(px(12.0));
        for &index in indexes {
            let Some(field) = UI_THEME_COLOR_FIELDS.get(index) else {
                continue;
            };
            let color = colors
                .get(index)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            grid = grid.child(self.theme_editor_color_cell(
                field,
                color,
                SettingsInput::CustomThemeUiColor(index),
                cx,
            ));
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .pb(px(4.0))
                    .border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x66))
                    .text_size(px(THEME_EDITOR_SECTION_TITLE_SIZE))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t(title_key).to_uppercase()),
            )
            .child(grid)
            .into_any_element()
    }

    fn theme_editor_color_grid_for_fields(
        &self,
        fields: &[ThemeColorField],
        colors: &[String],
        section: ThemeEditorSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut grid = div()
            .w_full()
            .grid()
            .grid_cols(THEME_EDITOR_GRID_COLUMNS as u16)
            .gap_x(px(16.0))
            .gap_y(px(12.0));
        for (index, field) in fields.iter().enumerate() {
            let color = colors
                .get(index)
                .cloned()
                .unwrap_or_else(|| "#000000".to_string());
            let input = match section {
                ThemeEditorSection::Terminal => SettingsInput::CustomThemeTerminalColor(index),
                ThemeEditorSection::Ui => SettingsInput::CustomThemeUiColor(index),
            };
            grid = grid.child(self.theme_editor_color_cell(field, color, input, cx));
        }
        grid.into_any_element()
    }

    fn theme_editor_color_cell(
        &self,
        field: &ThemeColorField,
        color: String,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let parsed = parse_color_hex(&color).unwrap_or(0);
        let focused = self.focused_settings_input == Some(input);
        let label = self
            .i18n
            .t(&format!("settings_view.custom_theme.colors.{}", field.label_key));
        div()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .size(px(THEME_EDITOR_FIELD_SWATCH_SIZE))
                    .rounded(px(self.tokens.radii.sm))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | 0x99))
                    .bg(rgb(parsed))
                    .shadow_sm()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            let current = this.current_settings_input_value(input);
                            this.focus_settings_input(input, current, cx);
                            this.ime_marked_text = None;
                            window.focus(&this.focus_handle);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(THEME_EDITOR_SWATCH_LABEL_SIZE))
                            .line_height(px(12.0))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .truncate()
                            .child(label),
                    )
                    .child(if focused {
                        self.theme_editor_text_input(
                            input,
                            color,
                            "#RRGGBB".to_string(),
                            THEME_EDITOR_HEX_INPUT_WIDTH,
                            true,
                            false,
                            cx,
                        )
                    } else {
                        div()
                            .text_size(px(THEME_EDITOR_SWATCH_LABEL_SIZE))
                            .line_height(px(12.0))
                            .font_family(settings_mono_font_family(self.settings_store.settings()))
                            .text_color(rgba((self.tokens.ui.text << 8) | 0xb3))
                            .cursor(CursorStyle::IBeam)
                            .hover(|hex| hex.text_color(rgb(self.tokens.ui.accent)))
                            .child(color)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, window, cx| {
                                    let current = this.current_settings_input_value(input);
                                    this.focus_settings_input(input, current, cx);
                                    this.ime_marked_text = None;
                                    window.focus(&this.focus_handle);
                                    cx.stop_propagation();
                                }),
                            )
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    fn theme_editor_label(&self, key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(key))
            .into_any_element()
    }

    fn theme_editor_text_input(
        &self,
        input: SettingsInput,
        value: String,
        placeholder: String,
        width: f32,
        mono: bool,
        fill: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.as_str()
        } else {
            value.as_str()
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        let mut control = text_input(
            &self.tokens,
            TextInputView {
                value: display_value,
                placeholder,
                focused,
                caret_visible: self.new_connection_caret_visible,
                secret: false,
                selected_all: false,
                marked_text: self.marked_text_for_target(target),
            },
        )
        .h(px(THEME_EDITOR_INPUT_HEIGHT))
        .cursor(CursorStyle::IBeam)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, window, cx| {
                let current = this.current_settings_input_value(input);
                this.focus_settings_input(input, current, cx);
                this.ime_marked_text = None;
                window.focus(&this.focus_handle);
                cx.stop_propagation();
            }),
        );
        control = if fill {
            control.w_full()
        } else {
            control.w(px(width))
        };
        if mono {
            control = control.font_family(settings_mono_font_family(self.settings_store.settings()));
        }
        text_input_anchor_probe(
            target.anchor_id(),
            control,
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn theme_editor_footer_button(&self, icon: LucideIcon, label: String, color: u32) -> Div {
        div()
            .h(px(self.tokens.metrics.ui_button_sm_height))
            .px(px(self.tokens.metrics.ui_button_sm_padding_x))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((color << 8) | 0x4d))
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(color))
            .cursor_pointer()
            .hover(|style| style.bg(rgba((color << 8) | 0x1a)))
            .child(Self::render_lucide_icon(icon, 12.0, rgb(color)))
            .child(label)
    }

    fn open_theme_editor(&mut self, edit_theme_id: Option<String>, cx: &mut Context<Self>) {
        let settings = self.settings_store.settings();
        let fallback_id = if built_in_theme_exists(&settings.terminal.theme) {
            settings.terminal.theme.clone()
        } else {
            "azurite".to_string()
        };
        let (name, duplicate_theme, terminal, ui) = edit_theme_id
            .as_deref()
            .and_then(|theme_id| {
                let (terminal, ui) = custom_theme_terminal_and_ui(settings, theme_id)?;
                Some((
                    custom_theme_name(settings, theme_id)
                        .unwrap_or_else(|| self.i18n.t("settings_view.custom_theme.new_theme_name")),
                    fallback_id.clone(),
                    terminal,
                    ui,
                ))
            })
            .unwrap_or_else(|| {
                let duplicate_theme = fallback_id.clone();
                let terminal = theme_by_id(&duplicate_theme).terminal;
                let ui = derive_ui_colors_from_terminal(terminal);
                (
                    self.i18n.t("settings_view.custom_theme.new_theme_name"),
                    duplicate_theme,
                    terminal,
                    ui,
                )
            });

        self.theme_editor = Some(ThemeEditorState {
            edit_theme_id,
            name,
            duplicate_theme,
            duplicate_theme_touched: false,
            terminal_colors: terminal_theme_to_colors(terminal),
            ui_colors: app_ui_colors_to_colors(ui),
            active_section: ThemeEditorSection::Terminal,
        });
        self.open_settings_select = None;
        self.focused_settings_input = None;
        cx.notify();
    }

    fn close_theme_editor(&mut self, cx: &mut Context<Self>) {
        self.theme_editor = None;
        self.open_settings_select = None;
        self.focused_settings_input = None;
        cx.notify();
    }

    fn apply_theme_editor_color(
        &mut self,
        section: ThemeEditorSection,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let value = self.settings_input_draft.trim().to_string();
        let Some(editor) = self.theme_editor.as_mut() else {
            return;
        };
        let colors = match section {
            ThemeEditorSection::Terminal => &mut editor.terminal_colors,
            ThemeEditorSection::Ui => &mut editor.ui_colors,
        };
        if let Some(slot) = colors.get_mut(index) {
            *slot = value;
        }
        cx.notify();
    }

    fn save_theme_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.theme_editor.clone() else {
            return;
        };
        let name = editor.name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let theme_id = editor
            .edit_theme_id
            .unwrap_or_else(|| format!("{}{}", CUSTOM_THEME_PREFIX, slugify_custom_theme_name(&name)));
        let terminal = editor_terminal_theme(&editor.terminal_colors);
        let ui = editor_ui_colors(&editor.ui_colors);
        let value = custom_theme_json(&name, terminal, ui);
        let selected_theme_id = theme_id.clone();
        self.edit_settings(
            move |settings| {
                settings.custom_themes.insert(theme_id.clone(), value.clone());
                settings.terminal.theme = selected_theme_id.clone();
            },
            cx,
        );
        self.theme_editor = None;
        self.focused_settings_input = None;
        self.send_settings_notice(
            self.i18n
                .t("settings_view.appearance.theme_import_success")
                .replace("{{name}}", &name),
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    fn delete_theme_editor_theme(&mut self, cx: &mut Context<Self>) {
        let Some(theme_id) = self
            .theme_editor
            .as_ref()
            .and_then(|editor| editor.edit_theme_id.clone())
        else {
            return;
        };
        self.edit_settings(
            move |settings| {
                settings.custom_themes.remove(&theme_id);
                if settings.terminal.theme == theme_id {
                    settings.terminal.theme = "azurite".to_string();
                }
            },
            cx,
        );
        self.theme_editor = None;
        self.focused_settings_input = None;
        cx.notify();
    }

    fn import_theme_from_file(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.appearance.theme_import"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let result = fs::read_to_string(&path)
                .map_err(|err| err.to_string())
                .and_then(|contents| import_custom_theme(&contents));
            let _ = weak.update(cx, |this, cx| match result {
                Ok((theme_id, name, value)) => {
                    let selected_theme_id = theme_id.clone();
                    this.edit_settings(
                        move |settings| {
                            settings.custom_themes.insert(theme_id.clone(), value.clone());
                            settings.terminal.theme = selected_theme_id.clone();
                        },
                        cx,
                    );
                    this.send_settings_notice(
                        this.i18n
                            .t("settings_view.appearance.theme_import_success")
                            .replace("{{name}}", &name),
                        TerminalNoticeVariant::Success,
                    );
                }
                Err(error) => {
                    this.send_settings_notice(
                        this.i18n
                            .t("settings_view.appearance.theme_import_error")
                            .replace("{{error}}", &error),
                        TerminalNoticeVariant::Error,
                    );
                }
            });
        })
        .detach();
    }

    fn send_settings_notice(&self, title: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    fn appearance_background_gallery(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_row()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.terminal.bg_gallery")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                self.appearance_action_button(
                                    LucideIcon::Plus,
                                    self.i18n.t("settings_view.terminal.bg_add"),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.pick_background_image(cx);
                                        cx.stop_propagation();
                                    }),
                                ),
                            )
                            .when(settings.terminal.background_image.is_some(), |actions| {
                                actions.child(
                                    div()
                                        .h(px(self
                                            .tokens
                                            .metrics
                                            .settings_appearance_action_height))
                                        .px(px(10.0))
                                        .flex()
                                        .flex_row()
                                        .items_center()
                                        .gap(px(6.0))
                                        .rounded(px(self.tokens.radii.md))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .text_color(rgb(self.tokens.ui.error))
                                        .cursor_pointer()
                                        .hover(|style| {
                                            style.bg(rgba((self.tokens.ui.error << 8) | 0x14))
                                        })
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::Trash2,
                                            14.0,
                                            rgb(self.tokens.ui.error),
                                        ))
                                        .child(self.i18n.t("settings_view.terminal.bg_clear_all"))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.edit_settings(
                                                    |settings| {
                                                        settings.terminal.background_image = None;
                                                    },
                                                    cx,
                                                );
                                            }),
                                        ),
                                )
                            }),
                    ),
            )
            .child(self.background_thumbnails(settings, cx))
            .into_any_element()
    }

    fn background_thumbnails(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(current) = settings.terminal.background_image.as_deref() else {
            return div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.i18n.t("settings_view.terminal.bg_hint"))
                .into_any_element();
        };

        div()
            .w_full()
            .grid()
            .grid_cols(4)
            .gap(px(8.0))
            .child(self.background_thumbnail(current, true, cx))
            .into_any_element()
    }

    fn pick_background_image(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(SharedString::from(
                self.i18n.t("settings_view.terminal.bg_add"),
            )),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            if !is_supported_background_image(&path) {
                return;
            }
            let image_path = path.to_string_lossy().to_string();
            let _ = weak.update(cx, |this, cx| {
                this.edit_settings(
                    move |settings| {
                        settings.terminal.background_image = Some(image_path);
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    fn background_thumbnail(
        &self,
        image_path: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let image_path = image_path.to_string();
        let image_source = std::path::PathBuf::from(&image_path);
        let fallback_label = std::path::Path::new(&image_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(&image_path)
            .to_string();
        let fallback_text_size = self.tokens.metrics.ui_text_xs;
        let fallback_text_color = self.tokens.ui.text_muted;
        let fallback_icon_color = self.tokens.ui.text_muted;
        let fallback_bg = self.tokens.ui.bg_sunken;
        let thumbnail_radius = self.tokens.radii.md;
        let image = gpui::img(image_source)
            .w_full()
            .h_full()
            .rounded(px(thumbnail_radius))
            .object_fit(ObjectFit::Cover)
            .with_fallback(move || {
                div()
                    .w_full()
                    .h_full()
                    .rounded(px(thumbnail_radius))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(6.0))
                    .bg(rgb(fallback_bg))
                    .child(WorkspaceApp::render_lucide_icon(
                        LucideIcon::Image,
                        20.0,
                        rgb(fallback_icon_color),
                    ))
                    .child(
                        div()
                            .max_w_full()
                            .px(px(8.0))
                            .text_size(px(fallback_text_size))
                            .text_color(rgb(fallback_text_color))
                            .truncate()
                            .child(fallback_label.clone()),
                    )
                    .into_any_element()
            });

        let mut thumbnail = div()
            .relative()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .overflow_hidden()
            .border_2()
            .border_color(rgb(if active {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .cursor_pointer()
            .child(image);
        thumbnail.style().aspect_ratio = Some(BACKGROUND_THUMBNAIL_ASPECT_RATIO);
        thumbnail
            .when(active, |thumb| {
                thumb.child(
                    div()
                        .absolute()
                        .top(px(8.0))
                        .left(px(8.0))
                        .rounded(px(self.tokens.radii.sm))
                        .bg(rgb(self.tokens.ui.accent))
                        .px(px(self.tokens.metrics.settings_background_badge_padding_x))
                        .py(px(self.tokens.metrics.settings_background_badge_padding_y))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.accent_text))
                        .child(self.i18n.t("settings_view.terminal.bg_active")),
                )
            })
            .child(
                div()
                    .absolute()
                    .top(px(6.0))
                    .right(px(6.0))
                    .p(px(3.0))
                    .rounded(px(self.tokens.radii.sm))
                    .bg(rgba(0x00000099))
                    .text_color(rgb(self.tokens.ui.text))
                    .child(Self::render_lucide_icon(
                        LucideIcon::X,
                        12.0,
                        rgb(self.tokens.ui.text),
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.edit_settings(
                                |settings| {
                                    settings.terminal.background_image = None;
                                },
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    let selected_path = image_path.clone();
                    this.edit_settings(
                        move |settings| {
                            settings.terminal.background_image = Some(selected_path);
                        },
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
    fn appearance_background_tabs(
        &self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut grid = div().w_full().grid().grid_cols(3).gap(px(10.0));
        for (key, label_key, icon) in background_tab_options() {
            let enabled = settings
                .terminal
                .background_enabled_tabs
                .iter()
                .any(|tab| tab == key);
            let key = (*key).to_string();
            grid = grid.child(
                self.background_tab_pill(
                    &key,
                    *label_key,
                    settings_background_tab_lucide(*icon),
                    enabled,
                )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.toggle_background_tab(&key, cx);
                        }),
                    ),
            );
        }

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.i18n.t("settings_view.terminal.bg_tabs")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(self.i18n.t("settings_view.terminal.bg_tabs_hint")),
                    ),
            )
            .child(grid)
            .into_any_element()
    }

    fn background_tab_pill(
        &self,
        _key: &str,
        label_key: &str,
        icon: LucideIcon,
        enabled: bool,
    ) -> Div {
        div()
            .h(px(40.0))
            .min_w(px(0.0))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(10.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(if enabled {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.border
            }))
            .bg(if enabled {
                rgba((self.tokens.ui.accent << 8) | 0x1a)
            } else {
                rgba(0x00000000)
            })
            .px(px(14.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(if enabled {
                self.tokens.ui.accent
            } else {
                self.tokens.ui.text_muted
            }))
            .cursor_pointer()
            .child(Self::render_lucide_icon(
                icon,
                self.tokens.metrics.settings_background_tab_icon_size,
                rgb(if enabled {
                    self.tokens.ui.accent
                } else {
                    self.tokens.ui.text_muted
                }),
            ))
            .child(div().truncate().child(self.i18n.t(label_key)))
    }

    fn toggle_background_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        self.edit_settings(
            |settings| {
                if let Some(index) = settings
                    .terminal
                    .background_enabled_tabs
                    .iter()
                    .position(|tab| tab == key)
                {
                    settings.terminal.background_enabled_tabs.remove(index);
                } else {
                    settings
                        .terminal
                        .background_enabled_tabs
                        .push(key.to_string());
                }
            },
            cx,
        );
    }
}
