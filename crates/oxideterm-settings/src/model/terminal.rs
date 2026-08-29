#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct GeneralSettings {
    pub language: Language,
    pub update_channel: UpdateChannel,
    #[serde(
        rename = "minimizeToTrayOnClose",
        default = "default_minimize_to_tray_on_close"
    )]
    pub minimize_to_tray_on_close: bool,
    #[serde(
        rename = "externalConnectionUrisEnabled",
        default = "default_external_connection_uris_enabled"
    )]
    pub external_connection_uris_enabled: bool,
    #[serde(default)]
    pub update_proxy: UpdateProxySettings,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            language: Language::ZhCn,
            update_channel: UpdateChannel::default(),
            minimize_to_tray_on_close: default_minimize_to_tray_on_close(),
            external_connection_uris_enabled: default_external_connection_uris_enabled(),
            update_proxy: UpdateProxySettings::default(),
            extra: ExtraFields::new(),
        }
    }
}

fn default_minimize_to_tray_on_close() -> bool {
    true
}

fn default_external_connection_uris_enabled() -> bool {
    // External applications should not open connections until the user opts in.
    false
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
pub struct TerminalTriggerSettings {
    // Shell execution is a separate trust decision from enabling ordinary terminal triggers.
    #[serde(default)]
    pub explicit_shell_enabled: bool,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl Default for TerminalTriggerSettings {
    fn default() -> Self {
        Self {
            explicit_shell_enabled: false,
            extra: ExtraFields::new(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCommandBarSettings {
    pub enabled: bool,
    pub git_status: bool,
    #[serde(default = "default_command_bar_project_tasks")]
    pub project_tasks: bool,
    #[serde(default = "default_command_bar_current_directory_awareness")]
    pub current_directory_awareness: bool,
    #[serde(default = "default_command_bar_show_current_directory")]
    pub show_current_directory: bool,
    pub smart_completion: bool,
    pub quick_commands_enabled: bool,
    #[serde(default)]
    pub quick_bar_enabled: bool,
    pub quick_commands_confirm_before_run: bool,
    pub quick_commands_show_toast: bool,
    pub focus_handoff_commands: Vec<String>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// Commands that normally take over terminal input after launch.
pub const RECOMMENDED_FOCUS_HANDOFF_COMMANDS: &[&str] = &[
    "agy",
    "btop",
    "claude",
    "codex",
    "emacs",
    "fzf",
    "htop",
    "lazydocker",
    "lazygit",
    "less",
    "man",
    "micro",
    "nano",
    "nvim",
    "opencode",
    "ranger",
    "screen",
    "ssh",
    "tig",
    "tmux",
    "top",
    "vi",
    "vim",
    "yazi",
];

impl Default for TerminalCommandBarSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            git_status: true,
            project_tasks: true,
            current_directory_awareness: true,
            show_current_directory: true,
            smart_completion: true,
            quick_commands_enabled: true,
            quick_bar_enabled: false,
            quick_commands_confirm_before_run: false,
            quick_commands_show_toast: true,
            focus_handoff_commands: RECOMMENDED_FOCUS_HANDOFF_COMMANDS
                .iter()
                .map(|command| (*command).to_string())
                .collect(),
            extra: ExtraFields::new(),
        }
    }
}

fn default_command_bar_project_tasks() -> bool {
    true
}

fn default_command_bar_current_directory_awareness() -> bool {
    true
}

fn default_command_bar_show_current_directory() -> bool {
    true
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

fn default_terminal_smooth_scroll() -> bool {
    true
}

fn default_highlight_tab_on_new_output() -> bool {
    true
}

fn default_open_links_with_modifier() -> bool {
    // Terminal clicks commonly focus or select text, so opening links requires deliberate input.
    true
}

fn default_detect_file_paths_as_links() -> bool {
    true
}

fn default_terminal_semantic_coloring() -> bool {
    // Semantic coloring is opt-in because it changes application-provided terminal output.
    false
}

fn default_confirm_before_closing_ssh() -> bool {
    true
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalSemanticScheme {
    #[default]
    Balanced,
    Conservative,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalBroadcastTargetKind {
    Ssh,
    Mosh,
    Telnet,
    Serial,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalBroadcastTargetRef {
    // Pane IDs are runtime-only; the saved profile ID keeps membership stable across launches.
    pub kind: TerminalBroadcastTargetKind,
    pub saved_connection_id: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalBroadcastGroup {
    // The UUID keeps documents and the interactive broadcaster bound across renames.
    pub id: uuid::Uuid,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<TerminalBroadcastTargetRef>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TerminalSessionLogFileMode {
    #[default]
    Unique,
    Append,
    Overwrite,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TerminalSessionLogSettings {
    // Automatic logging remains opt-in because terminal output may contain sensitive data.
    pub automatic: bool,
    pub include_control_sequences: bool,
    pub retention_days: i64,
    pub max_file_size_mib: i64,
    pub file_name_template: String,
    pub content_template: String,
    pub file_mode: TerminalSessionLogFileMode,
}

impl Default for TerminalSessionLogSettings {
    fn default() -> Self {
        Self {
            automatic: false,
            include_control_sequences: false,
            retention_days: 30,
            max_file_size_mib: 100,
            file_name_template: "{date}_{time}_{protocol}_{session}.log".to_string(),
            content_template: "[{timestamp}] {text}".to_string(),
            file_mode: TerminalSessionLogFileMode::Unique,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSettings {
    pub theme: String,
    pub font_family: FontFamily,
    pub custom_font_family: String,
    #[serde(default)]
    pub cjk_font_family: String,
    pub font_size: i64,
    /// CSS-compatible weight requested for regular terminal text.
    #[serde(default = "default_terminal_font_weight")]
    pub font_weight: i64,
    // Terminal ligatures stay opt-in so existing monospace rendering remains stable.
    #[serde(default)]
    pub font_ligatures: bool,
    pub line_height: f64,
    pub cursor_style: CursorStyle,
    pub cursor_blink: bool,
    pub scrollback: i64,
    #[serde(default = "default_terminal_smooth_scroll")]
    pub smooth_scroll: bool,
    pub renderer: RendererType,
    pub terminal_encoding: TerminalEncoding,
    // Legacy terminal applications disagree on the bytes produced by these physical keys.
    #[serde(default)]
    pub backspace_sequence: TerminalBackspaceSequence,
    #[serde(default)]
    pub delete_sequence: TerminalDeleteSequence,
    pub adaptive_renderer: AdaptiveRendererMode,
    // Keep the legacy serialized field name so existing settings continue to load.
    pub show_fps_overlay: bool,
    // This controls transient tab chrome without changing terminal polling or session ownership.
    #[serde(default = "default_highlight_tab_on_new_output")]
    pub highlight_tab_on_new_output: bool,
    pub paste_protection: bool,
    pub smart_copy: bool,
    pub osc52_clipboard: bool,
    // Clipboard reads expose local data to remote programs, so legacy settings default to denied.
    #[serde(default)]
    pub osc52_clipboard_read: bool,
    pub copy_on_select: bool,
    pub middle_click_paste: bool,
    // Right-click paste stays opt-in because right click normally opens the
    // terminal context menu and can be reported to mouse-aware applications.
    #[serde(default)]
    pub right_click_paste: bool,
    #[serde(default = "default_open_links_with_modifier")]
    pub open_links_with_modifier: bool,
    #[serde(default = "default_detect_file_paths_as_links")]
    pub detect_file_paths_as_links: bool,
    // Existing installations keep the protective prompt until the user opts out.
    #[serde(default = "default_confirm_before_closing_ssh")]
    pub confirm_before_closing_ssh: bool,
    pub selection_requires_shift: bool,
    // Keep the legacy JSON key so local and cloud-synced settings remain compatible.
    #[serde(default, rename = "freeTypeCursorPositioning")]
    pub free_type_mode: bool,
    pub autosuggest: TerminalAutosuggestSettings,
    pub command_bar: TerminalCommandBarSettings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub broadcast_groups: Vec<TerminalBroadcastGroup>,
    #[serde(default)]
    pub session_log: TerminalSessionLogSettings,
    #[serde(default)]
    pub triggers: TerminalTriggerSettings,
    #[serde(default)]
    pub remote_shell_integration_mode: RemoteShellIntegrationMode,
    pub command_marks: TerminalCommandMarksSettings,
    pub background_enabled: bool,
    pub background_image: Option<String>,
    pub background_opacity: f64,
    pub background_blur: i64,
    pub background_fit: BackgroundFit,
    #[serde(default)]
    pub background_scope: BackgroundScope,
    pub background_enabled_tabs: Vec<String>,
    // Semantic coloring supplements only terminal cells without explicit ANSI styling.
    #[serde(default = "default_terminal_semantic_coloring")]
    pub semantic_coloring: bool,
    #[serde(default)]
    pub semantic_scheme: TerminalSemanticScheme,
    #[serde(default)]
    pub semantic_custom_scheme: Option<String>,
    #[serde(default)]
    pub custom_semantic_schemes: Vec<SemanticSchemeDocument>,
    pub highlight_rules: Vec<HighlightRule>,
    #[serde(default)]
    pub highlight_rule_sets: Vec<HighlightRuleSet>,
    #[serde(default)]
    pub default_highlight_rule_set: Option<String>,
    pub in_band_transfer: InBandTransferSettings,
    pub graphics: TerminalGraphicsSettings,
    pub unicode: TerminalUnicodeSettings,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

pub const DEFAULT_TERMINAL_BACKGROUND_OPACITY: f64 = 0.15;
pub const MIN_TERMINAL_BACKGROUND_OPACITY: f64 = 0.03;
pub const MAX_TERMINAL_BACKGROUND_OPACITY: f64 = 1.0;
pub const DEFAULT_TERMINAL_FONT_WEIGHT: i64 = 400;
pub const MIN_TERMINAL_FONT_WEIGHT: i64 = 100;
pub const MAX_TERMINAL_FONT_WEIGHT: i64 = 900;
pub const MAX_CUSTOM_SEMANTIC_SCHEMES: usize = 32;

const fn default_terminal_font_weight() -> i64 {
    DEFAULT_TERMINAL_FONT_WEIGHT
}

impl TerminalSettings {
    pub fn active_custom_semantic_scheme(&self) -> Option<&SemanticSchemeDocument> {
        let active_id = self.semantic_custom_scheme.as_deref()?;
        self.custom_semantic_schemes
            .iter()
            .find(|scheme| scheme.id == active_id)
    }

    pub fn highlight_rule_set(&self, id: &str) -> Option<&HighlightRuleSet> {
        self.highlight_rule_sets
            .iter()
            .find(|rule_set| rule_set.id == id)
    }

    pub fn effective_highlight_rules(&self) -> &[HighlightRule] {
        self.default_highlight_rule_set
            .as_deref()
            .and_then(|id| self.highlight_rule_set(id))
            .map(|rule_set| rule_set.rules.as_slice())
            .unwrap_or(&self.highlight_rules)
    }

    pub fn effective_highlight_rules_mut(&mut self) -> &mut Vec<HighlightRule> {
        let selected = self.default_highlight_rule_set.clone();
        if let Some(id) = selected
            && let Some(index) = self
                .highlight_rule_sets
                .iter()
                .position(|rule_set| rule_set.id == id)
        {
            return &mut self.highlight_rule_sets[index].rules;
        }
        &mut self.highlight_rules
    }

    pub fn default_highlight_rule_set_name(&self) -> Option<&str> {
        self.default_highlight_rule_set
            .as_deref()
            .and_then(|id| self.highlight_rule_set(id))
            .map(|rule_set| rule_set.name.as_str())
    }
}

impl Default for TerminalSettings {
    fn default() -> Self {
        Self {
            theme: "default".to_string(),
            font_family: FontFamily::Jetbrains,
            custom_font_family: String::new(),
            cjk_font_family: String::new(),
            font_size: 14,
            font_weight: DEFAULT_TERMINAL_FONT_WEIGHT,
            font_ligatures: false,
            line_height: 1.2,
            cursor_style: CursorStyle::Block,
            cursor_blink: true,
            scrollback: DEFAULT_TERMINAL_SCROLLBACK,
            smooth_scroll: true,
            renderer: RendererType::default(),
            terminal_encoding: TerminalEncoding::Utf8,
            backspace_sequence: TerminalBackspaceSequence::default(),
            delete_sequence: TerminalDeleteSequence::default(),
            adaptive_renderer: AdaptiveRendererMode::Auto,
            show_fps_overlay: false,
            highlight_tab_on_new_output: true,
            paste_protection: true,
            smart_copy: true,
            osc52_clipboard: true,
            osc52_clipboard_read: false,
            copy_on_select: false,
            middle_click_paste: false,
            right_click_paste: false,
            open_links_with_modifier: true,
            detect_file_paths_as_links: true,
            confirm_before_closing_ssh: true,
            selection_requires_shift: false,
            free_type_mode: false,
            autosuggest: TerminalAutosuggestSettings::default(),
            command_bar: TerminalCommandBarSettings::default(),
            broadcast_groups: Vec::new(),
            session_log: TerminalSessionLogSettings::default(),
            triggers: TerminalTriggerSettings::default(),
            remote_shell_integration_mode: RemoteShellIntegrationMode::Ask,
            command_marks: TerminalCommandMarksSettings::default(),
            background_enabled: true,
            background_image: None,
            background_opacity: DEFAULT_TERMINAL_BACKGROUND_OPACITY,
            background_blur: 0,
            background_fit: BackgroundFit::Cover,
            background_scope: BackgroundScope::Content,
            background_enabled_tabs: vec!["terminal".to_string(), "local_terminal".to_string()],
            semantic_coloring: false,
            semantic_scheme: TerminalSemanticScheme::default(),
            semantic_custom_scheme: None,
            custom_semantic_schemes: Vec::new(),
            highlight_rules: Vec::new(),
            highlight_rule_sets: Vec::new(),
            default_highlight_rule_set: None,
            in_band_transfer: InBandTransferSettings::default(),
            graphics: TerminalGraphicsSettings::default(),
            unicode: TerminalUnicodeSettings::default(),
            extra: ExtraFields::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_scope_defaults_to_content_for_legacy_settings() {
        let mut value = serde_json::to_value(TerminalSettings::default()).expect("settings value");
        value
            .as_object_mut()
            .expect("terminal settings object")
            .remove("backgroundScope");
        let settings: TerminalSettings = serde_json::from_value(value).expect("legacy settings");

        assert_eq!(settings.background_scope, BackgroundScope::Content);
    }

    #[test]
    fn terminal_trigger_shell_execution_defaults_to_denied() {
        let mut value = serde_json::to_value(TerminalSettings::default()).expect("settings value");
        value
            .as_object_mut()
            .expect("terminal settings object")
            .remove("triggers");

        let settings: TerminalSettings = serde_json::from_value(value).expect("legacy settings");

        assert!(!settings.triggers.explicit_shell_enabled);
    }

    #[test]
    fn terminal_broadcast_groups_default_empty_for_legacy_settings() {
        let mut value = serde_json::to_value(TerminalSettings::default()).expect("settings value");
        value
            .as_object_mut()
            .expect("terminal settings object")
            .remove("broadcastGroups");

        let settings: TerminalSettings = serde_json::from_value(value).expect("legacy settings");

        assert!(settings.broadcast_groups.is_empty());
    }

    #[test]
    fn terminal_session_log_uses_safe_defaults_when_section_is_absent() {
        let mut value = serde_json::to_value(TerminalSettings::default()).expect("settings value");
        value
            .as_object_mut()
            .expect("terminal settings object")
            .remove("sessionLog");

        let settings: TerminalSettings = serde_json::from_value(value).expect("terminal settings");

        assert!(!settings.session_log.automatic);
        assert!(!settings.session_log.include_control_sequences);
        assert_eq!(settings.session_log.retention_days, 30);
        assert_eq!(settings.session_log.max_file_size_mib, 100);
        assert_eq!(
            settings.session_log.file_name_template,
            "{date}_{time}_{protocol}_{session}.log"
        );
        assert_eq!(
            settings.session_log.content_template,
            "[{timestamp}] {text}"
        );
        assert_eq!(
            settings.session_log.file_mode,
            TerminalSessionLogFileMode::Unique
        );
    }

    #[test]
    fn terminal_broadcast_groups_round_trip_stable_connection_ids() {
        let group_id = uuid::Uuid::parse_str("018f5d5e-7b6c-7ef0-a765-32109abcdef0").unwrap();
        let mut settings = TerminalSettings::default();
        settings.broadcast_groups.push(TerminalBroadcastGroup {
            id: group_id,
            name: "Production".to_string(),
            members: vec![TerminalBroadcastTargetRef {
                kind: TerminalBroadcastTargetKind::Ssh,
                saved_connection_id: "ssh-production-1".to_string(),
            }],
        });

        let value = serde_json::to_value(&settings).expect("serialize terminal settings");
        let restored: TerminalSettings =
            serde_json::from_value(value).expect("deserialize terminal settings");

        assert_eq!(restored.broadcast_groups, settings.broadcast_groups);
    }

    #[test]
    fn terminal_settings_restore_legacy_presentation_defaults() {
        let defaults: [(&str, bool, fn(&TerminalSettings) -> bool); 6] = [
            ("smoothScroll", true, |settings| settings.smooth_scroll),
            ("highlightTabOnNewOutput", true, |settings| {
                settings.highlight_tab_on_new_output
            }),
            (
                "freeTypeCursorPositioning",
                false,
                |settings| settings.free_type_mode,
            ),
            ("fontLigatures", false, |settings| settings.font_ligatures),
            ("rightClickPaste", false, |settings| settings.right_click_paste),
            ("semanticColoring", false, |settings| {
                settings.semantic_coloring
            }),
        ];

        for (field, expected, read) in defaults {
            let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
            value.as_object_mut().unwrap().remove(field);

            let settings: TerminalSettings = serde_json::from_value(value).unwrap();
            assert_eq!(read(&settings), expected, "legacy {field} default");
        }
    }

    #[test]
    fn terminal_semantic_scheme_defaults_and_serializes_stably() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("semanticScheme");

        let legacy: TerminalSettings = serde_json::from_value(value).unwrap();
        assert_eq!(legacy.semantic_scheme, TerminalSemanticScheme::Balanced);

        let mut conservative = TerminalSettings::default();
        conservative.semantic_scheme = TerminalSemanticScheme::Conservative;
        let value = serde_json::to_value(conservative).unwrap();
        assert_eq!(value["semanticScheme"], serde_json::json!("conservative"));
    }

    #[test]
    fn custom_semantic_schemes_round_trip_and_resolve_by_id() {
        let mut scheme = oxideterm_terminal_semantic::built_in_scheme_document(
            oxideterm_terminal_semantic::SemanticScheme::Balanced,
        );
        scheme.id = "custom:operations".to_string();
        scheme.name = "Operations".to_string();

        let mut settings = TerminalSettings::default();
        settings.semantic_custom_scheme = Some(scheme.id.clone());
        settings.custom_semantic_schemes.push(scheme.clone());
        let value = serde_json::to_value(settings).unwrap();
        let decoded: TerminalSettings = serde_json::from_value(value).unwrap();

        assert_eq!(decoded.active_custom_semantic_scheme(), Some(&scheme));
    }

    #[test]
    fn selected_highlight_rule_set_replaces_global_base_rules() {
        let mut settings = TerminalSettings::default();
        settings.highlight_rules.push(HighlightRule {
            id: "base".to_string(),
            ..HighlightRule::default()
        });
        settings.highlight_rule_sets.push(HighlightRuleSet {
            id: "operations".to_string(),
            name: "Operations".to_string(),
            rules: vec![HighlightRule {
                id: "override".to_string(),
                ..HighlightRule::default()
            }],
        });

        assert_eq!(settings.effective_highlight_rules()[0].id, "base");
        settings.default_highlight_rule_set = Some("operations".to_string());
        assert_eq!(settings.effective_highlight_rules()[0].id, "override");
        settings.effective_highlight_rules_mut()[0].label = "edited".to_string();
        assert_eq!(settings.highlight_rule_sets[0].rules[0].label, "edited");
    }

    #[test]
    fn terminal_settings_default_legacy_key_sequences_when_missing() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        let object = value.as_object_mut().unwrap();
        object.remove("backspaceSequence");
        object.remove("deleteSequence");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();

        assert_eq!(
            settings.backspace_sequence,
            TerminalBackspaceSequence::Delete
        );
        assert_eq!(settings.delete_sequence, TerminalDeleteSequence::Csi3Tilde);
    }

    #[test]
    fn terminal_settings_serialize_legacy_key_sequences() {
        let mut settings = TerminalSettings::default();
        settings.backspace_sequence = TerminalBackspaceSequence::ControlH;
        settings.delete_sequence = TerminalDeleteSequence::Delete;

        let value = serde_json::to_value(settings).expect("serialize terminal settings");

        assert_eq!(value["backspaceSequence"], serde_json::json!("controlH"));
        assert_eq!(value["deleteSequence"], serde_json::json!("delete"));
    }

    #[test]
    fn terminal_settings_keep_legacy_free_type_mode_json_key() {
        let mut settings = TerminalSettings::default();
        settings.free_type_mode = true;

        let value = serde_json::to_value(settings).expect("serialize terminal settings");

        assert_eq!(
            value["freeTypeCursorPositioning"],
            serde_json::Value::Bool(true)
        );
        assert!(value.get("freeTypeMode").is_none());
    }

    #[test]
    fn terminal_settings_default_osc52_clipboard_read_when_missing() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("osc52ClipboardRead");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();

        assert!(!settings.osc52_clipboard_read);
    }

    #[test]
    fn terminal_settings_confirm_ssh_close_for_legacy_settings() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("confirmBeforeClosingSsh");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();

        assert!(settings.confirm_before_closing_ssh);
    }

    #[test]
    fn terminal_settings_require_modifier_for_links_when_setting_is_missing() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("openLinksWithModifier");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();

        // Missing settings retain the safer native behavior that avoids accidental link opens.
        assert!(settings.open_links_with_modifier);
    }

    #[test]
    fn terminal_settings_detect_file_paths_when_setting_is_missing() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("detectFilePathsAsLinks");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();

        // Existing installations retain file path recognition until the user disables it.
        assert!(settings.detect_file_paths_as_links);
    }

    #[test]
    fn terminal_settings_ask_before_remote_shell_integration_when_missing() {
        let mut value = serde_json::to_value(TerminalSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("remoteShellIntegrationMode");

        let settings: TerminalSettings = serde_json::from_value(value).unwrap();

        assert_eq!(
            settings.remote_shell_integration_mode,
            RemoteShellIntegrationMode::Ask
        );
    }

    #[test]
    fn command_bar_settings_restore_legacy_defaults() {
        let defaults: [(&str, bool, fn(&TerminalCommandBarSettings) -> bool); 3] = [
            (
                "currentDirectoryAwareness",
                true,
                |settings| settings.current_directory_awareness,
            ),
            ("projectTasks", true, |settings| settings.project_tasks),
            ("quickBarEnabled", false, |settings| settings.quick_bar_enabled),
        ];

        for (field, expected, read) in defaults {
            let mut value = serde_json::to_value(TerminalCommandBarSettings::default()).unwrap();
            value.as_object_mut().unwrap().remove(field);

            let settings: TerminalCommandBarSettings = serde_json::from_value(value).unwrap();
            assert_eq!(read(&settings), expected, "legacy {field} default");
        }
    }
}
