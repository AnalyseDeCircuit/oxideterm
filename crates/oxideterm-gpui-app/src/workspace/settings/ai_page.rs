const AI_PROVIDER_SECTION_BORDER_ALPHA: u32 = 0xb3; // Tauri border-theme-border/70.
const AI_PROVIDER_SECTION_BG_ALPHA: u32 = 0x99; // Tauri bg-theme-bg/60.
const AI_PROVIDER_MODEL_BORDER_ALPHA: u32 = 0x80; // Tauri border-theme-border/50.
const AI_PROVIDER_MODEL_ACTIVE_BG_ALPHA: u32 = 0x1a; // Tauri bg-theme-accent/10.
const AI_PROVIDER_MODEL_ACTIVE_BORDER_ALPHA: u32 = 0x99; // Tauri border-theme-accent/60.
const AI_PROVIDER_SELECT_W: f32 = 224.0; // Tauri w-56.
const AI_PROVIDER_MAX_W: f32 = 768.0; // Tauri max-w-3xl.
const AI_PROVIDER_VISIBLE_MODEL_LIMIT: usize = 8;
const AI_CONTEXT_MAX_CHAR_OPTIONS: [i64; 5] = [2_000, 4_000, 8_000, 16_000, 32_000];
const AI_CONTEXT_VISIBLE_LINE_OPTIONS: [i64; 4] = [50, 100, 200, 400];
const AI_CONTEXT_NUMBER_W: f32 = 112.0; // Tauri w-28.
const AI_CONFIRM_DIALOG_WIDTH: f32 = 448.0; // Tauri DialogContent max-w-md.
const AI_KEY_REMOVE_DIALOG_WIDTH: f32 = 384.0; // Tauri useConfirm max-w-sm.
const AI_CONFIRM_BULLET_SIZE: f32 = 4.0; // Tauri w-1 h-1.
const AI_CONFIRM_ICON_WRAP: f32 = 48.0; // Tauri useConfirm w-12 h-12.
const AI_CONFIRM_ICON: f32 = 24.0; // Tauri useConfirm w-6 h-6.

include!("ai/surface.rs");
include!("ai/sections.rs");
include!("ai/mcp.rs");
include!("ai/provider_card.rs");
include!("ai/provider_keys.rs");
include!("ai/provider_actions.rs");
include!("ai/dialogs.rs");
include!("ai/provider_add.rs");
include!("ai/helpers.rs");
