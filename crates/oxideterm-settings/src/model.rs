// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

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
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Stable,
    #[default]
    Beta,
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

impl FontFamily {
    pub fn terminal_family_name(self, custom: &str) -> String {
        if self == Self::Custom && !custom.trim().is_empty() {
            return custom.trim().to_string();
        }
        match self {
            Self::Jetbrains => "JetBrainsMono Nerd Font".to_string(),
            Self::Meslo => "MesloLGS Nerd Font Mono".to_string(),
            Self::Maple => "Maple Mono NF CN".to_string(),
            Self::Cascadia => "Cascadia Code".to_string(),
            Self::Consolas => "Consolas".to_string(),
            Self::Menlo => "Menlo".to_string(),
            Self::Custom => "JetBrainsMono Nerd Font".to_string(),
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HighlightRule {
    pub id: String,
    pub label: String,
    pub pattern: String,
    pub is_regex: bool,
    pub case_sensitive: bool,
    pub foreground: Option<String>,
    pub background: Option<String>,
    #[serde(default)]
    pub render_mode: HighlightRuleRenderMode,
    pub enabled: bool,
    pub priority: i64,
}

impl Default for HighlightRule {
    fn default() -> Self {
        Self {
            id: "highlight-rule-1".to_string(),
            label: String::new(),
            pattern: String::new(),
            is_regex: false,
            case_sensitive: false,
            foreground: Some("#f8fafc".to_string()),
            background: Some("#991b1b".to_string()),
            render_mode: HighlightRuleRenderMode::Background,
            enabled: true,
            priority: 1,
        }
    }
}

#[derive(Clone)]
struct HighlightRuleCandidate {
    rule: HighlightRule,
    sort_priority: i64,
    index: usize,
}

pub fn create_default_highlight_rule(overrides: impl FnOnce(&mut HighlightRule)) -> HighlightRule {
    let mut rule = HighlightRule::default();
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    rule.id = format!("highlight-rule-1-{millis:x}");
    overrides(&mut rule);
    sanitize_highlight_rules(vec![rule])
        .into_iter()
        .next()
        .unwrap_or_default()
}

pub fn sanitize_highlight_rules(input: Vec<HighlightRule>) -> Vec<HighlightRule> {
    let mut seen_ids = std::collections::HashSet::new();
    let candidates = input
        .into_iter()
        .take(MAX_HIGHLIGHT_RULES)
        .enumerate()
        .map(|(index, mut rule)| {
            rule.id = rule.id.trim().to_string();
            if rule.id.is_empty() || seen_ids.contains(&rule.id) {
                rule.id = format!("highlight-rule-{}", index + 1);
            }
            seen_ids.insert(rule.id.clone());
            rule.label = rule.label.trim().to_string();
            rule.pattern = rule.pattern.trim().to_string();
            rule.foreground = sanitize_foreground_color(rule.foreground.as_deref());
            rule.background = sanitize_background_color(rule.background.as_deref());
            rule.priority = rule.priority.clamp(1, MAX_HIGHLIGHT_RULES as i64);
            HighlightRuleCandidate {
                sort_priority: rule.priority,
                rule,
                index,
            }
        })
        .collect::<Vec<_>>();
    normalize_highlight_priorities(candidates)
}

pub fn reindex_highlight_rules(input: Vec<HighlightRule>) -> Vec<HighlightRule> {
    let mut rules = sanitize_highlight_rules(input);
    let total = rules.len() as i64;
    for (index, rule) in rules.iter_mut().enumerate() {
        rule.priority = total - index as i64;
    }
    rules
}

pub fn sanitize_highlight_rules_value(input: &Value) -> Value {
    let Ok(rules) = serde_json::from_value::<Vec<HighlightRule>>(input.clone()) else {
        return json!([]);
    };
    json!(sanitize_highlight_rules(rules))
}

fn normalize_highlight_priorities(candidates: Vec<HighlightRuleCandidate>) -> Vec<HighlightRule> {
    let mut sorted = candidates.clone();
    sorted.sort_by(|left, right| {
        right
            .sort_priority
            .cmp(&left.sort_priority)
            .then_with(|| left.index.cmp(&right.index))
    });

    let highest = sorted.len() as i64;
    let priority_by_id = sorted
        .iter()
        .enumerate()
        .map(|(index, candidate)| (candidate.rule.id.clone(), highest - index as i64))
        .collect::<std::collections::HashMap<_, _>>();

    candidates
        .into_iter()
        .map(|mut candidate| {
            candidate.rule.priority = *priority_by_id.get(&candidate.rule.id).unwrap_or(&1);
            candidate.rule
        })
        .collect()
}

fn sanitize_foreground_color(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty() && is_hex_color(value, false)).then(|| value.to_string())
}

fn sanitize_background_color(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    (is_hex_color(value, true)
        || color_function_like(value, "rgb")
        || color_function_like(value, "rgba")
        || color_function_like(value, "hsl")
        || color_function_like(value, "hsla")
        || value.starts_with("var(--") && value.ends_with(')'))
    .then(|| value.to_string())
}

fn is_hex_color(value: &str, allow_short: bool) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    let valid_len = matches!(hex.len(), 6 | 8) || (allow_short && matches!(hex.len(), 3 | 4));
    valid_len && hex.chars().all(|ch| ch.is_ascii_hexdigit())
}

fn color_function_like(value: &str, name: &str) -> bool {
    value
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('('))
        .is_some_and(|rest| rest.ends_with(')'))
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneralSettings {
    pub language: Language,
    pub update_channel: UpdateChannel,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: Language::ZhCn,
            update_channel: UpdateChannel::Beta,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalAutosuggestSettings {
    pub local_shell_history: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalAutosuggestSettings {
    fn default() -> Self {
        Self {
            local_shell_history: true,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandBarSettings {
    pub enabled: bool,
    pub show_legacy_toolbar: bool,
    pub git_status: bool,
    pub smart_completion: bool,
    pub quick_commands_enabled: bool,
    pub quick_commands_confirm_before_run: bool,
    pub quick_commands_show_toast: bool,
    pub focus_handoff_commands: Vec<String>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalCommandBarSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            show_legacy_toolbar: false,
            git_status: true,
            smart_completion: true,
            quick_commands_enabled: true,
            quick_commands_confirm_before_run: false,
            quick_commands_show_toast: true,
            focus_handoff_commands: [
                "vim", "nvim", "vi", "nano", "emacs", "less", "more", "top", "htop", "btop",
                "yazi", "ranger", "lf", "lazygit", "tmux", "screen", "ssh", "python", "node",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandMarksSettings {
    pub enabled: bool,
    pub user_input_observed: bool,
    pub heuristic_detection: bool,
    pub show_hover_actions: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalCommandMarksSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            user_input_observed: false,
            heuristic_detection: false,
            show_hover_actions: true,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InBandTransferSettings {
    pub enabled: bool,
    pub provider: String,
    pub allow_directory: bool,
    pub max_chunk_bytes: i64,
    pub max_file_count: i64,
    pub max_total_bytes: i64,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for InBandTransferSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: "trzsz".to_string(),
            allow_directory: true,
            max_chunk_bytes: 1024 * 1024,
            max_file_count: 1024,
            max_total_bytes: 10 * 1024 * 1024 * 1024,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalGraphicsSettings {
    pub enabled: bool,
    pub sixel: bool,
    pub iterm2_inline: bool,
    pub kitty: bool,
    pub pixel_limit: i64,
    pub storage_limit_mb: i64,
    pub show_placeholder: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalGraphicsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            sixel: true,
            iterm2_inline: true,
            kitty: true,
            pixel_limit: 16_777_216,
            storage_limit_mb: 16,
            show_placeholder: true,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalUnicodeSettings {
    pub bidi_enabled: bool,
    pub rtl_debug_overlay: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalUnicodeSettings {
    fn default() -> Self {
        Self {
            bidi_enabled: true,
            rtl_debug_overlay: false,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub theme: String,
    pub font_family: FontFamily,
    pub custom_font_family: String,
    pub font_size: i64,
    pub line_height: f64,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub scrollback: i64,
    pub renderer: RendererType,
    pub terminal_encoding: TerminalEncoding,
    pub adaptive_renderer: AdaptiveRendererMode,
    pub show_fps_overlay: bool,
    pub paste_protection: bool,
    pub smart_copy: bool,
    pub osc52_clipboard: bool,
    pub copy_on_select: bool,
    pub middle_click_paste: bool,
    pub selection_requires_shift: bool,
    pub autosuggest: TerminalAutosuggestSettings,
    pub command_bar: TerminalCommandBarSettings,
    pub command_marks: TerminalCommandMarksSettings,
    pub background_enabled: bool,
    pub background_image: Option<String>,
    pub background_opacity: f64,
    pub background_blur: i64,
    pub background_fit: BackgroundFit,
    pub background_enabled_tabs: Vec<String>,
    pub highlight_rules: Vec<HighlightRule>,
    pub in_band_transfer: InBandTransferSettings,
    pub graphics: TerminalGraphicsSettings,
    pub unicode: TerminalUnicodeSettings,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            font_family: FontFamily::Jetbrains,
            custom_font_family: String::new(),
            font_size: 14,
            line_height: 1.2,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            scrollback: DEFAULT_TERMINAL_SCROLLBACK,
            renderer: RendererType::default(),
            terminal_encoding: TerminalEncoding::Utf8,
            adaptive_renderer: AdaptiveRendererMode::Auto,
            show_fps_overlay: false,
            paste_protection: true,
            smart_copy: true,
            osc52_clipboard: true,
            copy_on_select: false,
            middle_click_paste: false,
            selection_requires_shift: false,
            autosuggest: TerminalAutosuggestSettings::default(),
            command_bar: TerminalCommandBarSettings::default(),
            command_marks: TerminalCommandMarksSettings::default(),
            background_enabled: true,
            background_image: None,
            background_opacity: 0.15,
            background_blur: 0,
            background_fit: BackgroundFit::Cover,
            background_enabled_tabs: vec!["terminal".to_string(), "local_terminal".to_string()],
            highlight_rules: Vec::new(),
            in_band_transfer: InBandTransferSettings::default(),
            graphics: TerminalGraphicsSettings::default(),
            unicode: TerminalUnicodeSettings::default(),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiMemorySettings {
    pub enabled: bool,
    pub content: String,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiMemorySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            content: String::new(),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolUseSettings {
    pub enabled: bool,
    pub auto_approve_tools: Map<String, Value>,
    pub disabled_tools: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rounds: Option<i64>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiToolUseSettings {
    fn default() -> Self {
        let mut auto_approve_tools = Map::new();
        for (name, enabled) in [
            ("list_targets", true),
            ("select_target", true),
            ("observe_terminal", true),
            ("read_resource", true),
            ("get_state", true),
            ("recall_preferences", true),
            ("connect_target", false),
            ("run_command", false),
            ("send_terminal_input", false),
            ("write_resource", false),
            ("write_resource:settings", false),
            ("write_resource:file", false),
            ("transfer_resource", false),
            ("open_app_surface", false),
            ("remember_preference", false),
        ] {
            auto_approve_tools.insert(name.to_string(), json!(enabled));
        }
        Self {
            enabled: false,
            auto_approve_tools,
            disabled_tools: Vec::new(),
            max_rounds: Some(DEFAULT_AI_TOOL_MAX_ROUNDS),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiContextSources {
    pub ide: bool,
    pub sftp: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiContextSources {
    fn default() -> Self {
        Self {
            ide: true,
            sftp: true,
            extra: ExtraFields::new(),
        }
    }
}

fn default_execution_profiles() -> Value {
    json!({
        "defaultProfileId": "default",
        "profiles": [{
            "id": "default",
            "name": "Default",
            "providerId": null,
            "model": null,
            "reasoningEffort": "auto",
            "toolUse": {
                "enabled": false,
                "maxRounds": DEFAULT_AI_TOOL_MAX_ROUNDS,
                "autoApproveTools": {},
                "disabledTools": []
            },
            "context": {
                "includeRuntimeChips": true,
                "includeMemory": true,
                "includeRag": true
            },
            "commandPolicy": { "allow": [], "deny": [] },
            "createdAt": 0,
            "updatedAt": 0
        }]
    })
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiSettings {
    pub enabled: bool,
    pub enabled_confirmed: bool,
    pub base_url: String,
    pub model: String,
    pub providers: Vec<Value>,
    pub active_provider_id: Option<String>,
    pub active_model: Option<String>,
    pub context_max_chars: i64,
    pub context_visible_lines: i64,
    pub thinking_style: AiThinkingStyle,
    pub reasoning_effort: AiReasoningEffort,
    pub reasoning_provider_overrides: Map<String, Value>,
    pub reasoning_model_overrides: Map<String, Value>,
    pub thinking_default_expanded: bool,
    #[serde(default)]
    pub model_context_windows: Map<String, Value>,
    #[serde(default)]
    pub user_context_windows: Map<String, Value>,
    pub custom_system_prompt: String,
    pub memory: AiMemorySettings,
    #[serde(default)]
    pub model_max_response_tokens: Map<String, Value>,
    pub tool_use: AiToolUseSettings,
    pub context_sources: AiContextSources,
    #[serde(default)]
    pub mcp_servers: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub embedding_config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_roles: Option<Value>,
    pub execution_profiles: Value,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            enabled_confirmed: false,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "gpt-4o-mini".to_string(),
            providers: Vec::new(),
            active_provider_id: None,
            active_model: None,
            context_max_chars: 8000,
            context_visible_lines: 120,
            thinking_style: AiThinkingStyle::Detailed,
            reasoning_effort: AiReasoningEffort::Auto,
            reasoning_provider_overrides: Map::new(),
            reasoning_model_overrides: Map::new(),
            thinking_default_expanded: false,
            model_context_windows: Map::new(),
            user_context_windows: Map::new(),
            custom_system_prompt: String::new(),
            memory: AiMemorySettings::default(),
            model_max_response_tokens: Map::new(),
            tool_use: AiToolUseSettings::default(),
            context_sources: AiContextSources::default(),
            mcp_servers: Vec::new(),
            embedding_config: None,
            agent_roles: None,
            execution_profiles: default_execution_profiles(),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalTerminalSettings {
    pub default_shell_id: Option<String>,
    pub recent_shell_ids: Vec<String>,
    pub default_cwd: Option<String>,
    pub git_bash_path: Option<String>,
    pub load_shell_profile: bool,
    pub oh_my_posh_enabled: bool,
    pub oh_my_posh_theme: Option<String>,
    pub custom_env_vars: Map<String, Value>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for LocalTerminalSettings {
    fn default() -> Self {
        Self {
            default_shell_id: None,
            recent_shell_ids: Vec::new(),
            default_cwd: None,
            git_bash_path: None,
            load_shell_profile: true,
            oh_my_posh_enabled: false,
            oh_my_posh_theme: None,
            custom_env_vars: Map::new(),
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SftpSettings {
    pub max_concurrent_transfers: i64,
    pub directory_parallelism: i64,
    pub speed_limit_enabled: bool,
    pub speed_limit_kbps: i64,
    pub conflict_action: ConflictAction,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for SftpSettings {
    fn default() -> Self {
        Self {
            max_concurrent_transfers: 3,
            directory_parallelism: 4,
            speed_limit_enabled: false,
            speed_limit_kbps: 0,
            conflict_action: ConflictAction::Ask,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeSettings {
    pub auto_save: bool,
    pub font_size: Option<i64>,
    pub line_height: Option<f64>,
    pub agent_mode: IdeAgentMode,
    pub word_wrap: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for IdeSettings {
    fn default() -> Self {
        Self {
            auto_save: false,
            font_size: None,
            line_height: None,
            agent_mode: IdeAgentMode::Ask,
            word_wrap: false,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconnectSettings {
    pub enabled: bool,
    pub max_attempts: i64,
    pub base_delay_ms: i64,
    pub max_delay_ms: i64,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for ReconnectSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            max_attempts: 5,
            base_delay_ms: 1000,
            max_delay_ms: 15_000,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionPoolSettings {
    pub idle_timeout_secs: i64,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for ConnectionPoolSettings {
    fn default() -> Self {
        Self {
            idle_timeout_secs: 1800,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalSettings {
    pub virtual_session_proxy: bool,
    pub gpu_canvas: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for ExperimentalSettings {
    fn default() -> Self {
        Self {
            virtual_session_proxy: false,
            gpu_canvas: false,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeybindingSettings {
    pub overrides: Map<String, Value>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherSettings {
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewConnectionSettings {
    pub save_connection: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedSettings {
    pub version: u32,
    pub general: GeneralSettings,
    pub terminal: TerminalSettings,
    pub buffer: BufferSettings,
    pub appearance: AppearanceSettings,
    pub connection_defaults: ConnectionDefaults,
    #[serde(rename = "treeUI")]
    pub tree_ui: TreeUiState,
    #[serde(rename = "sidebarUI")]
    pub sidebar_ui: SidebarUiState,
    pub ai: AiSettings,
    pub local_terminal: LocalTerminalSettings,
    pub sftp: SftpSettings,
    pub ide: IdeSettings,
    pub reconnect: ReconnectSettings,
    pub connection_pool: ConnectionPoolSettings,
    pub experimental: ExperimentalSettings,
    pub onboarding_completed: bool,
    #[serde(default)]
    pub command_palette_mru: Vec<String>,
    #[serde(default)]
    pub keybindings: KeybindingSettings,
    #[serde(default)]
    pub custom_themes: Map<String, Value>,
    #[serde(default)]
    pub launcher: LauncherSettings,
    #[serde(default)]
    pub agent_roles: Option<Value>,
    #[serde(default)]
    pub new_connection: NewConnectionSettings,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_SCHEMA_VERSION,
            general: GeneralSettings::default(),
            terminal: TerminalSettings::default(),
            buffer: BufferSettings::default(),
            appearance: AppearanceSettings::default(),
            connection_defaults: ConnectionDefaults::default(),
            tree_ui: TreeUiState::default(),
            sidebar_ui: SidebarUiState::default(),
            ai: AiSettings::default(),
            local_terminal: LocalTerminalSettings::default(),
            sftp: SftpSettings::default(),
            ide: IdeSettings::default(),
            reconnect: ReconnectSettings::default(),
            connection_pool: ConnectionPoolSettings::default(),
            experimental: ExperimentalSettings::default(),
            onboarding_completed: false,
            command_palette_mru: Vec::new(),
            keybindings: KeybindingSettings::default(),
            custom_themes: Map::new(),
            launcher: LauncherSettings::default(),
            agent_roles: None,
            new_connection: NewConnectionSettings::default(),
            extra: ExtraFields::new(),
        }
    }
}

impl PersistedSettings {
    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("settings should serialize")
    }
}
