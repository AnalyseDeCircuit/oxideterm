pub use oxideterm_render_policy::RenderProfile;

pub const SETTINGS_SCHEMA_VERSION: u32 = 3;
pub const DEFAULT_TERMINAL_SCROLLBACK: i64 = 1000;
pub const TERMINAL_SCROLLBACK_MIN: i64 = 500;
pub const TERMINAL_SCROLLBACK_MAX: i64 = 20_000;
pub const DEFAULT_BACKEND_HOT_BUFFER_LINES: i64 = 8_000;
pub const BACKEND_HOT_BUFFER_MIN: i64 = 5_000;
pub const BACKEND_HOT_BUFFER_MAX: i64 = 12_000;
pub const DEFAULT_AI_TOOL_MAX_ROUNDS: i64 = 10;
pub const MIN_AI_TOOL_MAX_ROUNDS: i64 = 1;
pub const MAX_AI_TOOL_MAX_ROUNDS: i64 = 30;
pub const DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND: i64 = 8;
pub const MIN_AI_TOOL_MAX_CALLS_PER_ROUND: i64 = 1;
pub const MAX_AI_TOOL_MAX_CALLS_PER_ROUND: i64 = 32;
pub const MAX_HIGHLIGHT_RULES: usize = 32;
pub const MAX_HIGHLIGHT_PATTERN_LENGTH: usize = 512;

pub type ExtraFields = Map<String, Value>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum Language {
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "fr-FR")]
    FrFr,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "es-ES")]
    EsEs,
    #[serde(rename = "pt-BR")]
    PtBr,
    #[serde(rename = "vi")]
    Vi,
    #[serde(rename = "ko")]
    Ko,
    #[serde(rename = "de")]
    De,
    #[serde(rename = "it")]
    It,
    #[serde(rename = "zh-TW")]
    ZhTw,
}

impl Default for Language {
    fn default() -> Self {
        Self::ZhCn
    }
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::En => "en",
            Self::FrFr => "fr-FR",
            Self::Ja => "ja",
            Self::EsEs => "es-ES",
            Self::PtBr => "pt-BR",
            Self::Vi => "vi",
            Self::Ko => "ko",
            Self::De => "de",
            Self::It => "it",
            Self::ZhTw => "zh-TW",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateChannel {
    Stable,
    Beta,
    #[default]
    GpuiPreview,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RendererType {
    Auto,
    Webgl,
    Canvas,
}

impl Default for RendererType {
    fn default() -> Self {
        if cfg!(windows) {
            Self::Canvas
        } else {
            Self::Auto
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum AdaptiveRendererMode {
    #[default]
    Auto,
    Always60,
    Off,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum TerminalEncoding {
    #[default]
    #[serde(rename = "utf-8")]
    Utf8,
    #[serde(rename = "gbk")]
    Gbk,
    #[serde(rename = "gb18030")]
    Gb18030,
    #[serde(rename = "big5")]
    Big5,
    #[serde(rename = "shift_jis")]
    ShiftJis,
    #[serde(rename = "euc-jp")]
    EucJp,
    #[serde(rename = "euc-kr")]
    EucKr,
    #[serde(rename = "windows-1252")]
    Windows1252,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FontFamily {
    #[default]
    Jetbrains,
    Meslo,
    Maple,
    Cascadia,
    Consolas,
    Menlo,
    Custom,
}

pub const JETBRAINS_MONO_SUBSET_FAMILY: &str = "JetBrainsMono Nerd Font Mono";
pub const MESLO_SUBSET_FAMILY: &str = "MesloLGLDZ Nerd Font Mono";
pub const MAPLE_MONO_SUBSET_FAMILY: &str = "Maple Mono NF CN";

impl FontFamily {
    pub fn terminal_family_name(self, custom: &str) -> String {
        if self == Self::Custom && !custom.trim().is_empty() {
            return custom.trim().to_string();
        }
        match self {
            Self::Jetbrains => JETBRAINS_MONO_SUBSET_FAMILY.to_string(),
            Self::Meslo => MESLO_SUBSET_FAMILY.to_string(),
            Self::Maple => MAPLE_MONO_SUBSET_FAMILY.to_string(),
            Self::Cascadia => "Cascadia Code".to_string(),
            Self::Consolas => "Consolas".to_string(),
            Self::Menlo => "Menlo".to_string(),
            Self::Custom => JETBRAINS_MONO_SUBSET_FAMILY.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorStyle {
    #[default]
    Block,
    Underline,
    Bar,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundFit {
    #[default]
    Cover,
    Contain,
    Fill,
    Tile,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum UiDensity {
    Compact,
    #[default]
    Comfortable,
    Spacious,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AnimationSpeed {
    Off,
    Reduced,
    #[default]
    Normal,
    Fast,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FrostedGlassMode {
    #[default]
    Off,
    Css,
    Native,
    System,
    Mica,
    Acrylic,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictAction {
    #[default]
    Ask,
    Overwrite,
    Skip,
    Rename,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum IdeAgentMode {
    #[default]
    Ask,
    Enabled,
    Disabled,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AiThinkingStyle {
    #[default]
    Detailed,
    Compact,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AiReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
    #[default]
    Auto,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum HighlightRuleRenderMode {
    #[default]
    Background,
    Underline,
    Outline,
}
