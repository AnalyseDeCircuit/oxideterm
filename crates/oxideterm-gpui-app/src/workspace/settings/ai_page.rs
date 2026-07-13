use super::*;

pub(in crate::workspace) const AI_PROVIDER_SECTION_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70.
pub(in crate::workspace) const AI_PROVIDER_MODEL_BORDER_ALPHA: u32 = 0x80; // Tauri border-theme-border/50.
pub(in crate::workspace) const AI_PROVIDER_MODEL_ACTIVE_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10.
pub(in crate::workspace) const AI_PROVIDER_MODEL_ACTIVE_BORDER_ALPHA: u32 = 0x99; // Tauri border-theme-accent/60.
pub(in crate::workspace) const AI_PROVIDER_SELECT_W: f32 = 224.0; // Tauri w-56.
pub(in crate::workspace) const AI_PROVIDER_MAX_W: f32 = 768.0; // Tauri max-w-3xl.
pub(in crate::workspace) const AI_PROVIDER_VISIBLE_MODEL_LIMIT: usize = 8;
pub(in crate::workspace) const AI_CONTEXT_MAX_CHAR_OPTIONS: [i64; 5] =
    [2_000, 4_000, 8_000, 16_000, 32_000];
pub(in crate::workspace) const AI_CONTEXT_VISIBLE_LINE_OPTIONS: [i64; 4] = [50, 100, 200, 400];
pub(in crate::workspace) const AI_CONTEXT_NUMBER_W: f32 = 112.0; // Tauri w-28.
pub(in crate::workspace) const AI_CONFIRM_DIALOG_WIDTH: f32 = 448.0; // Tauri DialogContent max-w-md.
pub(in crate::workspace) const AI_KEY_REMOVE_DIALOG_WIDTH: f32 = 384.0; // Tauri useConfirm max-w-sm.
pub(in crate::workspace) const AI_CONFIRM_BULLET_SIZE: f32 = 4.0; // Tauri w-1 h-1.
pub(in crate::workspace) const AI_CONFIRM_ICON_WRAP: f32 = 48.0; // Tauri useConfirm w-12 h-12.
pub(in crate::workspace) const AI_CONFIRM_ICON: f32 = 24.0; // Tauri useConfirm w-6 h-6.
pub(in crate::workspace) const AI_TEXT_EDITOR_MODAL_WIDTH: f32 = 760.0;
pub(in crate::workspace) const AI_TEXT_EDITOR_MODAL_HEIGHT: f32 = 640.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiTextEditorDialog {
    SystemPrompt,
    Memory,
}

#[path = "ai/dialogs.rs"]
mod dialogs;
#[path = "ai/helpers.rs"]
mod helpers;
#[path = "ai/mcp.rs"]
mod mcp;
#[path = "ai/provider_actions.rs"]
mod provider_actions;
#[path = "ai/provider_add.rs"]
mod provider_add;
#[path = "ai/provider_card.rs"]
mod provider_card;
#[path = "ai/provider_keys.rs"]
mod provider_keys;
#[path = "ai/sections.rs"]
mod sections;
#[path = "ai/surface.rs"]
mod surface;
