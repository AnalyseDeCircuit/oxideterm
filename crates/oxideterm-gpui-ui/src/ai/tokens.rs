use gpui::{Rgba, rgba};
use oxideterm_theme::ThemeTokens;

pub(super) const AI_TW_RED: u32 = 0xef4444;
pub(super) const AI_TW_AMBER: u32 = 0xf59e0b;
pub(super) const AI_TW_YELLOW: u32 = 0xeab308;
pub(super) const AI_TW_GREEN: u32 = 0x22c55e;
pub(super) const AI_TW_EMERALD: u32 = 0x10b981;
pub(super) const AI_TW_SKY: u32 = 0x0ea5e9;
pub(super) const AI_TW_BLUE: u32 = 0x3b82f6;
pub(super) const AI_TW_ORANGE: u32 = 0xf97316;
pub(super) const AI_TW_VIOLET: u32 = 0x8b5cf6;

pub(super) const AI_PANEL_BORDER_ALPHA: u32 = 0x80; // Tauri border-theme-border/50.
pub(super) const AI_HEADER_BORDER_ALPHA: u32 = 0x4d; // Tauri border-theme-border/30.
pub(super) const AI_CHAT_INPUT_BORDER_ALPHA: u32 = 0x66; // Tauri border-theme-border/40.
pub(super) const AI_CHAT_INPUT_PANEL_ALPHA: u32 = 0x26; // Tauri bg-theme-bg-panel/15.
pub(super) const AI_CHAT_INPUT_FOOTER_BORDER_ALPHA: u32 = 0x1a; // Tauri border-theme-border/10.
pub(super) const AI_HOVER_BG_ALPHA: u32 = 0x4d; // Tauri hover:bg-theme-bg-hover/30.
pub(super) const AI_MUTED_TEXT_25_ALPHA: f32 = 0.25;
pub(super) const AI_MUTED_TEXT_30_ALPHA: f32 = 0.30;
pub(super) const AI_MUTED_TEXT_40_ALPHA: f32 = 0.40;
pub(super) const AI_MUTED_TEXT_50_ALPHA: f32 = 0.50;
pub(super) const AI_MUTED_TEXT_60_ALPHA: f32 = 0.60;
pub(super) const AI_MUTED_TEXT_70_ALPHA: f32 = 0.70;
pub(super) const AI_MUTED_TEXT_80_ALPHA: f32 = 0.80;
pub(super) const AI_MUTED_TEXT_85_ALPHA: f32 = 0.85;
pub(super) const AI_CHIP_BG_ALPHA: u32 = 0x1a; // Tauri *-500/10.
pub(super) const AI_CHIP_BORDER_ALPHA: u32 = 0x4d; // Tauri *-500/30.
pub(super) const AI_BLOCK_BG_ALPHA: u32 = 0x14; // Tauri *-500/8.
pub(super) const AI_BLOCK_BORDER_ALPHA: u32 = 0x40; // Tauri *-500/25.
pub(super) const AI_USER_BUBBLE_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10.
pub(super) const AI_USER_BUBBLE_BORDER_ALPHA: u32 = 0x33; // Tauri border-theme-accent/20.
pub(super) const AI_PRE_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg/60.
pub(super) const AI_TOOL_BG_ALPHA: u32 = 0x0d; // Tauri pending approval bg-*-500/5.
pub(super) const AI_TOOL_APPROVAL_BG_ALPHA: u32 = 0x33; // Tauri bg-green/red-500/20.
pub(super) const AI_TOOL_APPROVAL_HOVER_ALPHA: u32 = 0x4d; // Tauri hover:bg-green/red-500/30.
pub(super) const AI_MODEL_BADGE_BG_ALPHA: u32 = 0x8c; // Tauri bg-theme-bg-panel/55.
pub(super) const AI_CONTEXT_BAR_BG_ALPHA: u32 = 0x33; // Tauri bg-theme-border/20.

pub(super) const AI_INPUT_TEXT_SIZE: f32 = 13.0; // Tauri textarea text-[13px].
pub(super) const AI_TEXT_12: f32 = 12.0;
pub(super) const AI_TEXT_11: f32 = 11.0;
pub(super) const AI_TEXT_10: f32 = 10.0;
pub(super) const AI_TEXT_9: f32 = 9.0;
pub(super) const AI_ICON_16: f32 = 16.0;
pub(super) const AI_ICON_14: f32 = 14.0;
pub(super) const AI_ICON_12: f32 = 12.0;
pub(super) const AI_ICON_10: f32 = 10.0;
pub(super) const AI_SIDEBAR_HEADER_HEIGHT: f32 = 41.0; // Tauri px-3 py-2 header with text-sm.
pub(super) const AI_CHAT_INPUT_MIN_HEIGHT: f32 = 36.0; // Tauri textarea min-h-[36px].
pub(super) const AI_AUTOCOMPLETE_MAX_HEIGHT: f32 = 200.0; // Tauri max-h-[200px].
pub(super) const AI_THINKING_MAX_HEIGHT: f32 = 300.0; // Tauri ThinkingBlock max-h-[300px].
pub(super) const AI_GUARDRAIL_RAW_MAX_HEIGHT: f32 = 220.0; // Tauri max-h-[220px].
pub(super) const AI_TOOL_ARGS_MAX_HEIGHT: f32 = 120.0; // Tauri tool args max-h-[120px].
pub(super) const AI_TOOL_STRUCTURED_MAX_HEIGHT: f32 = 160.0; // Tauri structured data max-h-[160px].
pub(super) const AI_TOOL_OUTPUT_MAX_HEIGHT: f32 = 200.0; // Tauri raw output max-h-[200px].
pub(super) const AI_CONTEXT_POPOVER_WIDTH: f32 = 240.0; // Tauri w-60.
pub(super) const AI_CONTEXT_MINI_BAR_WIDTH: f32 = 64.0; // Tauri sm:w-16.
pub(super) const AI_CONTEXT_MINI_BAR_HEIGHT: f32 = 4.0; // Tauri h-1.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiTone {
    Accent,
    Amber,
    Blue,
    Emerald,
    Green,
    Muted,
    Orange,
    Red,
    Sky,
    Violet,
    Yellow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiMessageRole {
    Assistant,
    User,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiToolStatus {
    Pending,
    PendingApproval,
    Approved,
    Running,
    Completed,
    Error,
    Rejected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiToolRisk {
    Read,
    WriteFile,
    ExecuteCommand,
    InteractiveInput,
    Destructive,
    NetworkExpose,
    SettingsChange,
    CredentialSensitive,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiSafetyMode {
    Default,
    Bypass,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiWarningKind {
    Warning,
    Error,
}

#[derive(Clone, Copy, Debug)]
pub struct AiContextUsage {
    pub percentage: f32,
    pub warning: bool,
    pub danger: bool,
}

#[derive(Clone, Debug)]
pub struct AiToolCallView {
    pub name: String,
    pub summary: String,
    pub status: AiToolStatus,
    pub risk: AiToolRisk,
    pub risk_label: String,
    pub capability: Option<String>,
    pub duration: Option<String>,
    pub pending_denied_command: bool,
    pub bypass_approval: bool,
    pub bypass_label: String,
}

pub(super) fn tone_color(tokens: &ThemeTokens, tone: AiTone) -> u32 {
    match tone {
        AiTone::Accent => tokens.ui.accent,
        AiTone::Amber => AI_TW_AMBER,
        AiTone::Blue => AI_TW_BLUE,
        AiTone::Emerald => AI_TW_EMERALD,
        AiTone::Green => AI_TW_GREEN,
        AiTone::Muted => tokens.ui.text_muted,
        AiTone::Orange => AI_TW_ORANGE,
        AiTone::Red => AI_TW_RED,
        AiTone::Sky => AI_TW_SKY,
        AiTone::Violet => AI_TW_VIOLET,
        AiTone::Yellow => AI_TW_YELLOW,
    }
}

pub(super) fn tone_bg(tokens: &ThemeTokens, tone: AiTone, alpha: u32) -> Rgba {
    rgba((tone_color(tokens, tone) << 8) | alpha)
}

pub(super) fn tone_border(tokens: &ThemeTokens, tone: AiTone, alpha: u32) -> Rgba {
    rgba((tone_color(tokens, tone) << 8) | alpha)
}

pub(super) fn bg_alpha(tokens: &ThemeTokens, color: u32, alpha: u32) -> Rgba {
    let _ = tokens;
    rgba((color << 8) | alpha)
}

pub(super) fn muted_text(tokens: &ThemeTokens, opacity: f32) -> Rgba {
    rgba((tokens.ui.text_muted << 8) | ((opacity.clamp(0.0, 1.0) * 255.0).round() as u32))
}

pub(super) fn ai_font_family() -> gpui::SharedString {
    crate::tauri_ui_font_family("")
}

pub(super) fn risk_tone(risk: AiToolRisk) -> AiTone {
    match risk {
        AiToolRisk::Read => AiTone::Sky,
        AiToolRisk::WriteFile => AiTone::Amber,
        AiToolRisk::ExecuteCommand => AiTone::Blue,
        AiToolRisk::InteractiveInput => AiTone::Violet,
        AiToolRisk::Destructive => AiTone::Red,
        AiToolRisk::NetworkExpose => AiTone::Orange,
        AiToolRisk::SettingsChange => AiTone::Yellow,
        AiToolRisk::CredentialSensitive => AiTone::Red,
    }
}

pub(super) fn status_tone(status: AiToolStatus) -> AiTone {
    match status {
        AiToolStatus::Pending => AiTone::Yellow,
        AiToolStatus::PendingApproval => AiTone::Amber,
        AiToolStatus::Approved | AiToolStatus::Running => AiTone::Accent,
        AiToolStatus::Completed => AiTone::Green,
        AiToolStatus::Error => AiTone::Red,
        AiToolStatus::Rejected => AiTone::Muted,
    }
}
pub fn ai_icon_size_large() -> f32 {
    AI_ICON_16
}

pub fn ai_icon_size_medium() -> f32 {
    AI_ICON_14
}

pub fn ai_icon_size_small() -> f32 {
    AI_ICON_12
}

pub fn ai_icon_size_xsmall() -> f32 {
    AI_ICON_10
}
