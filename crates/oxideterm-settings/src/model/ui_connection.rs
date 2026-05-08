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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub sidebar_collapsed_default: bool,
    pub ui_density: UiDensity,
    pub border_radius: i64,
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
            sidebar_collapsed_default: false,
            ui_density: UiDensity::Comfortable,
            border_radius: 6,
            ui_font_family: String::new(),
            animation_speed: AnimationSpeed::Normal,
            frosted_glass: FrostedGlassMode::Off,
            render_profile: RenderProfile::Auto,
            extra: ExtraFields::new(),
        }
    }
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidebarUiState {
    pub collapsed: bool,
    pub active_section: String,
    pub width: i64,
    pub ai_sidebar_collapsed: bool,
    pub ai_sidebar_width: i64,
    pub zen_mode: bool,
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
            ai_sidebar_width: 340,
            zen_mode: false,
            extra: ExtraFields::new(),
        }
    }
}

