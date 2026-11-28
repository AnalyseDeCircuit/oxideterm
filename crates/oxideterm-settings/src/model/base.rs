pub use oxideterm_render_policy::RenderProfile;

pub const SETTINGS_SCHEMA_VERSION: u32 = 4;
pub const DEFAULT_COLOR_THEME: &str = "default";
pub const DEFAULT_TERMINAL_SCROLLBACK: i64 = 10_000;
pub const TERMINAL_SCROLLBACK_MIN: i64 = 100;
// Keep a generous upper bound well within alacritty_terminal's Line(i32) range
// so history + viewport arithmetic cannot overflow.
pub const TERMINAL_SCROLLBACK_MAX: i64 = 10_000_000;
pub const DEFAULT_BACKEND_HOT_BUFFER_LINES: i64 = 8_000;
pub const BACKEND_HOT_BUFFER_MIN: i64 = 5_000;
pub const BACKEND_HOT_BUFFER_MAX: i64 = 12_000;
pub const DEFAULT_AI_TOOL_MAX_ROUNDS: i64 = 25;
pub const MIN_AI_TOOL_MAX_ROUNDS: i64 = 1;
pub const MAX_AI_TOOL_MAX_ROUNDS: i64 = 30;
pub const DEFAULT_AI_TOOL_MAX_CALLS_PER_ROUND: i64 = 8;
pub const MIN_AI_TOOL_MAX_CALLS_PER_ROUND: i64 = 1;
pub const MAX_AI_TOOL_MAX_CALLS_PER_ROUND: i64 = 32;
pub const MAX_HIGHLIGHT_RULES: usize = 32;
pub const MAX_HIGHLIGHT_PATTERN_LENGTH: usize = 512;

pub type ExtraFields = Map<String, Value>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[derive(Default)]
pub enum Language {
    #[serde(rename = "zh-CN")]
    #[default]
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

    /// Maps a single BCP 47 or POSIX locale tag (e.g. `pt-PT`, `fr-CA`, `zh_TW.UTF-8`)
    /// to the closest supported UI language, returning `None` when unsupported.
    pub fn from_locale_tag(tag: &str) -> Option<Self> {
        let base = tag.split('.').next()?.split('@').next()?.trim();
        if base.is_empty() {
            return None;
        }
        let normalized = base.to_ascii_lowercase();
        let mut subtags = normalized
            .split(['-', '_'])
            .filter(|segment| !segment.is_empty());
        let primary = subtags.next()?;
        let rest: Vec<&str> = subtags.collect();

        match primary {
            "zh" | "cmn" => {
                if rest.contains(&"hant") {
                    Some(Self::ZhTw)
                } else if rest.contains(&"hans") {
                    Some(Self::ZhCn)
                } else if rest.iter().any(|tag| matches!(*tag, "tw" | "hk" | "mo")) {
                    Some(Self::ZhTw)
                } else {
                    Some(Self::ZhCn)
                }
            }
            "yue" => {
                if rest.iter().any(|tag| matches!(*tag, "hans" | "cn")) {
                    Some(Self::ZhCn)
                } else {
                    Some(Self::ZhTw)
                }
            }
            "en" => Some(Self::En),
            "fr" => Some(Self::FrFr),
            "pt" => Some(Self::PtBr),
            "es" => Some(Self::EsEs),
            "ja" => Some(Self::Ja),
            "ko" => Some(Self::Ko),
            "vi" => Some(Self::Vi),
            "de" => Some(Self::De),
            "it" => Some(Self::It),
            _ => None,
        }
    }

    /// Resolves the first supported UI language from an ordered list of preferred
    /// locale tags, falling back to English when none match.
    pub fn from_locale_tags<'a>(tags: impl IntoIterator<Item = &'a str>) -> Self {
        tags.into_iter()
            .find_map(Self::from_locale_tag)
            .unwrap_or(Self::En)
    }
}

/// Detects the initial UI language from the OS preferred locale list,
/// falling back to related regional variants and ultimately English.
pub fn detect_system_language() -> Language {
    let locales: Vec<String> = sys_locale::get_locales().collect();
    Language::from_locale_tags(locales.iter().map(String::as_str))
}

pub fn is_prerelease_version(version: &str) -> bool {
    version_contains_prerelease_tag(version, &["alpha", "beta", "rc", "pre", "preview"])
}

pub fn default_update_channel_for_version(version: &str) -> UpdateChannel {
    if is_prerelease_version(version) {
        UpdateChannel::Beta
    } else {
        UpdateChannel::Stable
    }
}

fn version_contains_prerelease_tag(version: &str, tags: &[&str]) -> bool {
    let Some((_, prerelease)) = version.split_once('-') else {
        return false;
    };
    let prerelease = prerelease.to_ascii_lowercase();
    tags.iter().any(|tag| {
        prerelease == *tag
            || prerelease
                .strip_prefix(tag)
                .is_some_and(|suffix| suffix.starts_with('.') || suffix.starts_with('-'))
    })
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateChannel {
    Stable,
    Beta,
}

impl Default for UpdateChannel {
    fn default() -> Self {
        // Stable builds follow Stable while all prerelease builds follow Beta.
        default_update_channel_for_version(env!("CARGO_PKG_VERSION"))
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateProxyMode {
    #[default]
    Direct,
    Application,
    System,
    Custom,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateProxyProtocol {
    #[default]
    Http,
    Https,
    Socks5,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProxySettings {
    pub mode: UpdateProxyMode,
    pub protocol: UpdateProxyProtocol,
    pub host: String,
    pub port: u16,
    pub no_proxy: String,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for UpdateProxySettings {
    fn default() -> Self {
        Self {
            mode: UpdateProxyMode::Direct,
            protocol: UpdateProxyProtocol::Http,
            host: "127.0.0.1".to_string(),
            port: 7890,
            no_proxy: String::new(),
            extra: ExtraFields::new(),
        }
    }
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
#[serde(rename_all = "camelCase")]
pub enum TerminalBackspaceSequence {
    #[default]
    Delete,
    ControlH,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalDeleteSequence {
    #[default]
    Csi3Tilde,
    Delete,
    ControlH,
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

pub const JETBRAINS_MONO_SUBSET_FAMILY: &str = "JetBrainsMono NFM";
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

    /// Natural `(ascent + |descent| + line_gap) / units_per_em` ratio used for
    /// offline v3 -> v4 line-height migration when no GPUI text system exists.
    pub fn reference_natural_line_height_ratio(self) -> f64 {
        match self {
            Self::Jetbrains | Self::Maple => 1.32,
            Self::Meslo => 3184.0 / 2048.0,
            Self::Menlo => 2384.0 / 2048.0,
            Self::Cascadia | Self::Consolas | Self::Custom => 1.2,
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
pub enum BackgroundScope {
    #[default]
    Content,
    Window,
}

impl BackgroundScope {
    pub fn includes_content(self, enabled_tabs: &[String], key: &str) -> bool {
        self == Self::Content && enabled_tabs.iter().any(|tab| tab == key)
    }
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
pub enum RemoteShellIntegrationMode {
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
    Max,
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

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HighlightRuleMatchScope {
    #[default]
    Match,
    LogicalLine,
}

#[cfg(test)]
mod locale_tests {
    use super::Language;

    #[test]
    fn system_locale_fallback_resolves_regional_variants_and_defaults_to_english() {
        for (tag, expected) in [
            ("pt-PT", Some(Language::PtBr)),
            ("pt-BR", Some(Language::PtBr)),
            ("pt", Some(Language::PtBr)),
            ("fr-CA", Some(Language::FrFr)),
            ("fr-CH", Some(Language::FrFr)),
            ("fr-FR", Some(Language::FrFr)),
            ("es-MX", Some(Language::EsEs)),
            ("es-419", Some(Language::EsEs)),
            ("de-AT", Some(Language::De)),
            ("it-CH", Some(Language::It)),
            ("ja-JP", Some(Language::Ja)),
            ("ko-KR", Some(Language::Ko)),
            ("vi-VN", Some(Language::Vi)),
            ("en-GB", Some(Language::En)),
            ("zh-CN", Some(Language::ZhCn)),
            ("zh-Hans-SG", Some(Language::ZhCn)),
            ("zh_CN.UTF-8", Some(Language::ZhCn)),
            ("zh-TW", Some(Language::ZhTw)),
            ("zh-Hant-HK", Some(Language::ZhTw)),
            ("zh-MO", Some(Language::ZhTw)),
            ("yue-HK", Some(Language::ZhTw)),
            ("ru-RU", None),
            ("C", None),
            ("POSIX", None),
            ("", None),
        ] {
            assert_eq!(
                Language::from_locale_tag(tag),
                expected,
                "locale tag {tag:?}"
            );
        }

        assert_eq!(
            Language::from_locale_tags(["ru-RU", "fr-CA", "en-US"]),
            Language::FrFr
        );
        assert_eq!(
            Language::from_locale_tags(["ru-RU", "ar-SA"]),
            Language::En
        );
        assert_eq!(Language::from_locale_tags([]), Language::En);
    }
}
