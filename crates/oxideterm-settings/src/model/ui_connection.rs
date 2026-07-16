#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BufferSettings {
    pub max_lines: i64,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for BufferSettings {
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_BACKEND_HOT_BUFFER_LINES,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppIconVariant {
    #[default]
    Default,
    WhiteBlue,
    WhiteGraphite,
    WhiteGreen,
    WhitePurple,
    WhiteRed,
    #[serde(alias = "orange")]
    FilledOrange,
    #[serde(alias = "blue")]
    FilledBlue,
    #[serde(alias = "graphite")]
    FilledGraphite,
    #[serde(alias = "green")]
    FilledGreen,
    #[serde(alias = "purple")]
    FilledPurple,
    #[serde(alias = "red")]
    FilledRed,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettings {
    #[serde(default)]
    pub app_icon: AppIconVariant,
    pub sidebar_collapsed_default: bool,
    pub ui_density: UiDensity,
    pub border_radius: i64,
    #[serde(default = "default_ui_font_size")]
    pub ui_font_size: i64,
    pub ui_font_family: String,
    pub animation_speed: AnimationSpeed,
    pub frosted_glass: FrostedGlassMode,
    pub render_profile: RenderProfile,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            app_icon: AppIconVariant::default(),
            sidebar_collapsed_default: false,
            ui_density: UiDensity::Comfortable,
            border_radius: 6,
            ui_font_size: default_ui_font_size(),
            ui_font_family: String::new(),
            animation_speed: AnimationSpeed::Normal,
            frosted_glass: FrostedGlassMode::Off,
            render_profile: RenderProfile::Auto,
            extra: ExtraFields::new(),
        }
    }
}

pub const DEFAULT_UI_FONT_SIZE: i64 = 14;

fn default_ui_font_size() -> i64 {
    DEFAULT_UI_FONT_SIZE
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDefaults {
    pub username: String,
    pub port: i64,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for ConnectionDefaults {
    fn default() -> Self {
        Self {
            username: "root".to_string(),
            port: 22,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TreeUiState {
    pub expanded_ids: Vec<String>,
    pub focused_node_id: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

pub const AI_SIDEBAR_MIN_WIDTH: f32 = 280.0;
// Tauri clamps the OxideSens sidebar at 500px; wider markdown/tool output must
// scroll inside the panel instead of continuing to consume workspace width.
pub const AI_SIDEBAR_MAX_WIDTH: f32 = 500.0;
pub const AI_SIDEBAR_DEFAULT_WIDTH: i64 = 340;

fn default_show_app_lock_icon() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarUiState {
    pub collapsed: bool,
    pub active_section: String,
    pub width: i64,
    pub ai_sidebar_collapsed: bool,
    pub ai_sidebar_width: i64,
    pub zen_mode: bool,
    #[serde(default = "default_show_app_lock_icon")]
    pub show_app_lock_icon: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for SidebarUiState {
    fn default() -> Self {
        Self {
            collapsed: false,
            active_section: "sessions".to_string(),
            width: 300,
            ai_sidebar_collapsed: true,
            ai_sidebar_width: AI_SIDEBAR_DEFAULT_WIDTH,
            zen_mode: false,
            show_app_lock_icon: true,
            extra: ExtraFields::new(),
        }
    }
}
