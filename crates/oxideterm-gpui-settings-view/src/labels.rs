use oxideterm_gpui_platform::vibrancy::NativeVibrancyMode;
use oxideterm_i18n::I18n;
use oxideterm_render_policy::RenderProfile;
use oxideterm_settings::{
    AiReasoningEffort, AiThinkingStyle, AnimationSpeed, ConflictAction, FontFamily,
    FrostedGlassMode, IdeAgentMode, UiDensity,
};

pub fn conflict_label(action: ConflictAction, i18n: &I18n) -> String {
    match action {
        ConflictAction::Ask => i18n.t("settings_view.sftp.conflict_ask"),
        ConflictAction::Overwrite => i18n.t("settings_view.sftp.conflict_overwrite"),
        ConflictAction::Skip => i18n.t("settings_view.sftp.conflict_skip"),
        ConflictAction::Rename => i18n.t("settings_view.sftp.conflict_rename"),
    }
}

pub fn ide_agent_label(mode: IdeAgentMode, i18n: &I18n) -> String {
    match mode {
        IdeAgentMode::Ask => i18n.t("settings_view.ide.agent_mode_ask"),
        IdeAgentMode::Enabled => i18n.t("settings_view.ide.agent_mode_enabled"),
        IdeAgentMode::Disabled => i18n.t("settings_view.ide.agent_mode_disabled"),
    }
}

pub fn font_family_label(family: FontFamily) -> String {
    match family {
        FontFamily::Jetbrains => "JetBrains Mono NF (Subset) ✓".to_string(),
        FontFamily::Meslo => "MesloLGS NF (Subset) ✓".to_string(),
        FontFamily::Maple => "Maple Mono NF CN (Subset) ✓".to_string(),
        FontFamily::Cascadia => "Cascadia Code".to_string(),
        FontFamily::Consolas => "Consolas".to_string(),
        FontFamily::Menlo => "Menlo".to_string(),
        FontFamily::Custom => "Custom...".to_string(),
    }
}

pub fn density_label(density: UiDensity, i18n: &I18n) -> String {
    match density {
        UiDensity::Compact => i18n.t("settings_view.appearance.density_compact"),
        UiDensity::Comfortable => i18n.t("settings_view.appearance.density_comfortable"),
        UiDensity::Spacious => i18n.t("settings_view.appearance.density_spacious"),
    }
}

pub fn animation_label(speed: AnimationSpeed, i18n: &I18n) -> String {
    match speed {
        AnimationSpeed::Off => i18n.t("settings_view.appearance.animation_off"),
        AnimationSpeed::Reduced => i18n.t("settings_view.appearance.animation_reduced"),
        AnimationSpeed::Normal => i18n.t("settings_view.appearance.animation_normal"),
        AnimationSpeed::Fast => i18n.t("settings_view.appearance.animation_fast"),
    }
}

pub fn frosted_glass_mode_from_native(mode: NativeVibrancyMode) -> FrostedGlassMode {
    match mode {
        NativeVibrancyMode::Off => FrostedGlassMode::Off,
        NativeVibrancyMode::System => FrostedGlassMode::System,
        NativeVibrancyMode::Mica => FrostedGlassMode::Mica,
        NativeVibrancyMode::Acrylic => FrostedGlassMode::Acrylic,
    }
}

pub fn frosted_glass_label(mode: FrostedGlassMode, i18n: &I18n) -> String {
    match mode {
        FrostedGlassMode::Off | FrostedGlassMode::Css => {
            i18n.t("settings_view.appearance.frosted_glass_off")
        }
        FrostedGlassMode::Native | FrostedGlassMode::System => {
            i18n.t("settings_view.appearance.frosted_glass_native")
        }
        FrostedGlassMode::Mica => "Mica".to_string(),
        FrostedGlassMode::Acrylic => "Acrylic".to_string(),
    }
}

pub fn render_profile_options() -> &'static [RenderProfile] {
    &[
        RenderProfile::Auto,
        RenderProfile::Quality,
        RenderProfile::LowPower,
        RenderProfile::Compatibility,
    ]
}

pub fn render_profile_label(profile: RenderProfile, i18n: &I18n) -> String {
    match profile {
        RenderProfile::Auto => i18n.t("settings_view.appearance.render_profile_auto"),
        RenderProfile::Quality => i18n.t("settings_view.appearance.render_profile_quality"),
        RenderProfile::LowPower => i18n.t("settings_view.appearance.render_profile_low_power"),
        RenderProfile::Compatibility => {
            i18n.t("settings_view.appearance.render_profile_compatibility")
        }
    }
}

pub fn ai_thinking_label(style: AiThinkingStyle) -> String {
    format!("{style:?}")
}

pub fn ai_reasoning_label(effort: AiReasoningEffort) -> String {
    format!("{effort:?}")
}
