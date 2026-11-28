//! Non-sensitive display preferences shared across reloads of a browser workspace.
use oxideterm_settings::*;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundledWallpaper {
    Ambient,
    Nocturne,
    Verdant,
}

impl BundledWallpaper {
    pub const ALL: [Self; 3] = [Self::Ambient, Self::Nocturne, Self::Verdant];
    pub fn filename(self) -> &'static str {
        match self {
            Self::Ambient => "oxide-ambient-v1.png",
            Self::Nocturne => "oxide-nocturne-v1.webp",
            Self::Verdant => "oxide-verdant-v1.webp",
        }
    }
    pub fn selected(settings: &TerminalSettings) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|w| Some(w.filename()) == settings.background_image.as_deref())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WallpaperId {
    Bundled(BundledWallpaper),
    Imported(uuid::Uuid),
}

impl WallpaperId {
    pub fn selected(t: &TerminalSettings) -> Option<Self> {
        BundledWallpaper::selected(t)
            .map(Self::Bundled)
            .or_else(|| {
                uuid::Uuid::parse_str(
                    t.background_image
                        .as_deref()?
                        .strip_prefix("browser-wallpaper:")?,
                )
                .ok()
                .map(Self::Imported)
            })
    }
    pub fn key(self) -> String {
        match self {
            Self::Bundled(b) => b.filename().into(),
            Self::Imported(id) => format!("browser-wallpaper:{id}"),
        }
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct WallpaperSettings {
    pub image: WallpaperId,
    enabled: bool,
    pub opacity: f64,
    pub scope: BackgroundScope,
    enabled_tabs: Vec<String>,
}

impl WallpaperSettings {
    fn capture(settings: &TerminalSettings) -> Option<Self> {
        Some(Self {
            image: WallpaperId::selected(settings)?,
            enabled: settings.background_enabled,
            opacity: settings.background_opacity,
            scope: settings.background_scope,
            enabled_tabs: crate::background_tab_options()
                .iter()
                .filter(|(key, _, _)| settings.background_enabled_tabs.iter().any(|t| t == key))
                .map(|(key, _, _)| (*key).to_owned())
                .collect(),
        })
    }
    pub fn active(settings: &TerminalSettings) -> Option<Self> {
        settings
            .background_enabled
            .then(|| Self::capture(settings))
            .flatten()
    }
    pub fn includes(&self, key: &str) -> bool {
        self.scope == BackgroundScope::Window
            || self.scope.includes_content(&self.enabled_tabs, key)
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct BrowserPreferences {
    language: Language,
    onboarding_completed: bool,
    disclaimer_accepted: bool,
    ai_enabled: bool,
    ai_confirmed: bool,
    appearance: Appearance,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wallpaper: Option<WallpaperSettings>,
    terminal: Terminal,
    sidebar_ui: Rails,
    #[serde(default)]
    sftp: FileBrowser,
    custom_themes: Map<String, Value>,
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    keybindings: Map<String, Value>,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Appearance {
    theme: String,
    manual_scheme: Option<AppearanceScheme>,
    follow_system_appearance: bool,
    system_themes: SystemThemeSettings,
    ui_density: UiDensity,
    ui_font_family: String,
    ui_font_size: i64,
    border_radius: i64,
    corner_style: CornerStyle,
    animation_speed: AnimationSpeed,
    app_icon: AppIconVariant,
    zen_mode_keep_tabbar: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Terminal {
    theme: String,
    system_themes: SystemThemeSettings,
    font_family: FontFamily,
    custom_font_family: String,
    cjk_font_family: String,
    font_ligatures: bool,
    font_weight: i64,
    font_size: i64,
    line_height: f64,
    character_spacing: f64,
    cursor_style: CursorStyle,
    cursor_blink: bool,
    confirm_before_closing_ssh: bool,
    #[serde(default)]
    command_bar: CommandBar,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct CommandBar {
    enabled: bool,
    current_directory_awareness: bool,
    show_current_directory: bool,
}

impl From<&TerminalCommandBarSettings> for CommandBar {
    fn from(s: &TerminalCommandBarSettings) -> Self {
        Self {
            enabled: s.enabled,
            current_directory_awareness: s.current_directory_awareness,
            show_current_directory: s.show_current_directory,
        }
    }
}

impl Default for CommandBar {
    fn default() -> Self {
        Self::from(&TerminalCommandBarSettings::default())
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Rails {
    width: i64,
    ai_sidebar_width: i64,
    collapsed: bool,
    ai_sidebar_collapsed: bool,
    zen_mode: bool,
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct FileBrowser {
    #[serde(default)]
    conflict_action: ConflictAction,
    presentation: SftpPresentationPreference,
    view_mode: SftpSidebarViewMode,
    session_fraction: f32,
}

impl Default for FileBrowser {
    fn default() -> Self {
        Self::from(&SftpSettings::default())
    }
}
impl From<&SftpSettings> for FileBrowser {
    fn from(s: &SftpSettings) -> Self {
        Self {
            presentation: s.presentation,
            conflict_action: s.conflict_action,
            view_mode: s.sidebar_view_mode,
            session_fraction: s.sidebar_session_fraction,
        }
    }
}

impl BrowserPreferences {
    pub fn capture(settings: &PersistedSettings, keybindings: Map<String, Value>) -> Self {
        Self {
            language: settings.general.language,
            onboarding_completed: settings.onboarding_completed,
            disclaimer_accepted: settings.onboarding_disclaimer_accepted,
            ai_enabled: settings.ai.enabled,
            ai_confirmed: settings.ai.enabled_confirmed,
            appearance: Appearance {
                theme: settings.appearance.theme.clone(),
                manual_scheme: settings.appearance.manual_scheme,
                follow_system_appearance: settings.appearance.follow_system_appearance,
                system_themes: settings.appearance.system_themes.clone(),
                ui_density: settings.appearance.ui_density,
                ui_font_family: settings.appearance.ui_font_family.clone(),
                ui_font_size: settings.appearance.ui_font_size,
                border_radius: settings.appearance.border_radius,
                corner_style: settings.appearance.corner_style,
                animation_speed: settings.appearance.animation_speed,
                app_icon: settings.appearance.app_icon,
                zen_mode_keep_tabbar: settings.appearance.zen_mode_keep_tabbar,
            },
            wallpaper: WallpaperSettings::capture(&settings.terminal),
            terminal: Terminal {
                theme: settings.terminal.theme.clone(),
                system_themes: settings.terminal.system_themes.clone(),
                font_family: settings.terminal.font_family,
                custom_font_family: settings.terminal.custom_font_family.clone(),
                cjk_font_family: settings.terminal.cjk_font_family.clone(),
                font_ligatures: settings.terminal.font_ligatures,
                font_weight: settings.terminal.font_weight,
                font_size: settings.terminal.font_size,
                line_height: settings.terminal.line_height,
                character_spacing: settings.terminal.character_spacing,
                cursor_style: settings.terminal.cursor_style,
                cursor_blink: settings.terminal.cursor_blink,
                confirm_before_closing_ssh: settings.terminal.confirm_before_closing_ssh,
                command_bar: CommandBar::from(&settings.terminal.command_bar),
            },
            sidebar_ui: Rails {
                width: settings.sidebar_ui.width,
                ai_sidebar_width: settings.sidebar_ui.ai_sidebar_width,
                collapsed: settings.sidebar_ui.collapsed,
                ai_sidebar_collapsed: settings.sidebar_ui.ai_sidebar_collapsed,
                zen_mode: settings.sidebar_ui.zen_mode,
            },
            custom_themes: display_palettes(&settings.custom_themes),
            sftp: FileBrowser::from(&settings.sftp),
            keybindings,
        }
    }

    pub fn apply(self, settings: &mut PersistedSettings) {
        settings.general.language = self.language;
        settings.onboarding_completed = self.onboarding_completed && self.disclaimer_accepted;
        settings.onboarding_disclaimer_accepted = self.disclaimer_accepted;
        settings.ai.enabled = self.ai_enabled;
        settings.ai.enabled_confirmed = self.ai_confirmed;
        settings.appearance.theme = self.appearance.theme;
        settings.appearance.manual_scheme = self.appearance.manual_scheme;
        settings.appearance.follow_system_appearance = self.appearance.follow_system_appearance;
        settings.appearance.system_themes = self.appearance.system_themes;
        settings.appearance.ui_density = self.appearance.ui_density;
        settings.appearance.ui_font_family = self.appearance.ui_font_family;
        settings.appearance.ui_font_size = self.appearance.ui_font_size;
        settings.appearance.border_radius = self.appearance.border_radius;
        settings.appearance.corner_style = self.appearance.corner_style;
        settings.appearance.animation_speed = self.appearance.animation_speed;
        settings.appearance.app_icon = self.appearance.app_icon;
        settings.appearance.zen_mode_keep_tabbar = self.appearance.zen_mode_keep_tabbar;
        if let Some(wallpaper) = &self.wallpaper {
            settings.terminal.background_image = Some(wallpaper.image.key());
            settings.terminal.background_scope = wallpaper.scope;
            settings.terminal.background_enabled_tabs = wallpaper.enabled_tabs.clone();
            settings.terminal.background_enabled = wallpaper.enabled;
            settings.terminal.background_opacity = wallpaper.opacity.clamp(
                MIN_TERMINAL_BACKGROUND_OPACITY,
                MAX_TERMINAL_BACKGROUND_OPACITY,
            );
        }
        settings.terminal.theme = self.terminal.theme;
        settings.terminal.system_themes = self.terminal.system_themes;
        settings.terminal.font_family = self.terminal.font_family;
        settings.terminal.custom_font_family = self.terminal.custom_font_family;
        settings.terminal.cjk_font_family = self.terminal.cjk_font_family;
        settings.terminal.font_ligatures = self.terminal.font_ligatures;
        settings.terminal.font_weight = self.terminal.font_weight;
        settings.terminal.font_size = self.terminal.font_size;
        settings.terminal.line_height = self.terminal.line_height;
        settings.terminal.character_spacing = self.terminal.character_spacing;
        settings.terminal.cursor_style = self.terminal.cursor_style;
        settings.terminal.cursor_blink = self.terminal.cursor_blink;
        settings.terminal.confirm_before_closing_ssh = self.terminal.confirm_before_closing_ssh;
        settings.terminal.command_bar.enabled = self.terminal.command_bar.enabled;
        settings.terminal.command_bar.current_directory_awareness =
            self.terminal.command_bar.current_directory_awareness;
        settings.terminal.command_bar.show_current_directory =
            self.terminal.command_bar.show_current_directory;
        settings.sidebar_ui.width = self.sidebar_ui.width;
        settings.sidebar_ui.ai_sidebar_width = self.sidebar_ui.ai_sidebar_width;
        settings.sidebar_ui.collapsed = self.sidebar_ui.collapsed;
        settings.sidebar_ui.ai_sidebar_collapsed = self.sidebar_ui.ai_sidebar_collapsed;
        settings.sidebar_ui.zen_mode = self.sidebar_ui.zen_mode;
        settings.custom_themes = display_palettes(&self.custom_themes);
        settings.keybindings.overrides = self.keybindings;
        settings.sftp.presentation = self.sftp.presentation;
        settings.sftp.conflict_action = self.sftp.conflict_action;
        settings.sftp.sidebar_view_mode = self.sftp.view_mode;
        settings.sftp.sidebar_session_fraction = self.sftp.session_fraction.clamp(0.2, 0.75);
    }

    pub fn complete_onboarding(&mut self, enable_ai: bool) {
        self.onboarding_completed = true;
        self.disclaimer_accepted = true;
        self.ai_enabled = enable_ai;
        self.ai_confirmed = enable_ai;
    }

    pub fn set_rails(&mut self, left: (i64, bool), right: (i64, bool)) {
        (self.sidebar_ui.width, self.sidebar_ui.collapsed) = left;
        (
            self.sidebar_ui.ai_sidebar_width,
            self.sidebar_ui.ai_sidebar_collapsed,
        ) = right;
    }
}

fn display_palettes(themes: &Map<String, Value>) -> Map<String, Value> {
    // Background paths and other host-specific metadata do not belong in browser storage.
    fn colors(theme: &Value, key: &str, fields: &[crate::ThemeColorField]) -> Option<Value> {
        let source = theme.get(key)?;
        let palette: Option<Map<String, Value>> = fields
            .iter()
            .map(|field| {
                let value = source.get(field.json_key)?.as_str()?;
                crate::parse_color_hex(value)?;
                Some((field.json_key.to_owned(), Value::String(value.to_owned())))
            })
            .collect();
        palette.map(Value::Object)
    }
    themes.iter().filter_map(|(id, theme)| {
        Some((id.clone(), serde_json::json!({
            "version": crate::theme::CUSTOM_THEME_IMPORT_VERSION,
            "name": theme.get("name")?.as_str()?,
            "terminalColors": colors(theme, "terminalColors", crate::TERMINAL_THEME_COLOR_FIELDS)?,
            "uiColors": colors(theme, "uiColors", crate::UI_THEME_COLOR_FIELDS)?,
        })))
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxideterm_theme::{ThemeTokens, theme_by_id};

    #[test]
    fn browser_display_restores_the_selected_palette_without_storing_private_settings() {
        let mut settings = PersistedSettings::default();
        settings.general.language = Language::ZhCn;
        settings.appearance.theme = "custom:night".into();
        settings.terminal.font_size = 19;
        settings.sidebar_ui.collapsed = true;
        settings.sftp.presentation = SftpPresentationPreference::Sidebar;
        settings.sftp.sidebar_view_mode = SftpSidebarViewMode::Tree;
        settings.sftp.sidebar_session_fraction = 0.6;
        settings.sftp.conflict_action = ConflictAction::Rename;
        let secret = "synthetic-private-marker";
        settings.terminal.background_image = Some(format!("/home/{secret}/oxide-ambient-v1.png"));
        let unbundled =
            serde_json::to_string(&BrowserPreferences::capture(&settings, Map::new())).unwrap();
        assert!(!unbundled.contains(secret));
        let key = "browser-wallpaper:1bf392e8-9241-4eb0-8fa3-e19e02dac012";
        settings.terminal.background_image = Some(key.into());
        let imported =
            serde_json::to_value(BrowserPreferences::capture(&settings, Map::new())).unwrap();
        assert_eq!(
            imported["wallpaper"]["image"],
            "1bf392e8-9241-4eb0-8fa3-e19e02dac012"
        );
        let mut reloaded = PersistedSettings::default();
        serde_json::from_value::<BrowserPreferences>(imported)
            .unwrap()
            .apply(&mut reloaded);
        assert_eq!(reloaded.terminal.background_image.as_deref(), Some(key));
        settings.terminal.background_image = Some("oxide-verdant-v1.webp".into());
        settings.terminal.background_enabled = false;
        settings.terminal.background_opacity = 0.41;
        settings.terminal.background_scope = BackgroundScope::Content;
        settings.terminal.background_enabled_tabs = vec!["settings".into(), secret.into()];
        settings
            .ai
            .extra
            .insert("apiKey".into(), Value::from(secret));
        settings
            .appearance
            .extra
            .insert("extension".into(), Value::from(secret));
        let built_in = theme_by_id("azurite");
        let mut theme = crate::custom_theme_json(
            "Night",
            built_in.terminal,
            ThemeTokens::from_builtin(built_in).ui,
        );
        theme["background"] = serde_json::json!({"image": secret});
        settings.custom_themes.insert("custom:night".into(), theme);

        let keys = serde_json::json!({"terminal.scrollTop": {"mac": null, "other": {"key": "f6"}}});
        let mut browser = BrowserPreferences::capture(&settings, keys.as_object().unwrap().clone());
        browser.complete_onboarding(true);
        let json = serde_json::to_string(&browser).unwrap();
        assert!(!json.contains(secret));
        let mut restored = PersistedSettings::default();
        serde_json::from_str::<BrowserPreferences>(&json)
            .unwrap()
            .apply(&mut restored);
        assert!(restored.onboarding_completed && restored.onboarding_disclaimer_accepted);
        assert!(restored.ai.enabled && restored.ai.enabled_confirmed);
        assert_eq!(restored.general.language, Language::ZhCn);
        assert_eq!(restored.appearance.theme, "custom:night");
        assert_eq!(restored.terminal.font_size, 19);
        assert_eq!(
            restored.terminal.background_image.as_deref(),
            Some("oxide-verdant-v1.webp")
        );
        assert!(!restored.terminal.background_enabled);
        assert_eq!(restored.terminal.background_opacity, 0.41);
        assert_eq!(restored.terminal.background_scope, BackgroundScope::Content);
        assert_eq!(restored.terminal.background_enabled_tabs, ["settings"]);
        assert_eq!(
            restored.keybindings.overrides["terminal.scrollTop"],
            serde_json::json!({"mac":null,"other":{"key":"f6"}})
        );
        assert!(restored.sidebar_ui.collapsed);
        assert_eq!(
            restored.sftp.presentation,
            SftpPresentationPreference::Sidebar
        );
        assert_eq!(restored.sftp.sidebar_view_mode, SftpSidebarViewMode::Tree);
        assert_eq!(restored.sftp.sidebar_session_fraction, 0.6);
        assert_eq!(restored.sftp.conflict_action, ConflictAction::Rename);
        assert_eq!(restored.custom_themes["custom:night"]["name"], "Night");
        assert_eq!(
            restored.custom_themes["custom:night"]["uiColors"]["bg"],
            settings.custom_themes["custom:night"]["uiColors"]["bg"]
        );
    }
}
