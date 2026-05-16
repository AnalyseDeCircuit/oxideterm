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
const AI_CONTEXT_SOURCE_ROW_GAP: f32 = 12.0; // Tauri flex items-center gap-3.
const AI_CONTEXT_SOURCE_BADGE_TEXT_SIZE: f32 = 9.0; // Tauri text-[9px].
const AI_CONTEXT_SOURCE_BADGE_PX: f32 = 6.0; // Tauri px-1.5.
const AI_CONTEXT_SOURCE_BADGE_PY: f32 = 2.0; // Tauri py-0.5.
const AI_CONTEXT_SOURCE_USER_COLOR: u32 = 0x60a5fa; // Tauri text-blue-400.
const AI_CONTEXT_SOURCE_API_COLOR: u32 = 0x34d399; // Tauri text-emerald-400.
const AI_CONTEXT_SOURCE_NAME_COLOR: u32 = 0x22d3ee; // Tauri text-cyan-400.
const AI_CONTEXT_SOURCE_BADGE_BG_ALPHA: u32 = 0x1a; // Tauri bg-*-400/10.
const AI_CONTEXT_SOURCE_DEFAULT_TEXT_ALPHA: u32 = 0xb3; // Tauri text-theme-text-muted/70.
const AI_CONTEXT_SOURCE_DEFAULT_BG_ALPHA: u32 = 0x33; // Tauri bg-theme-border/20.
const AI_CONTEXT_PROVIDER_ROW_BORDER_ALPHA: u32 = 0x4d; // Tauri border-theme-border/30.
const AI_CONTEXT_PROVIDER_ROW_TOP_BORDER_ALPHA: u32 = 0x33; // Tauri border-theme-border/20.
const AI_CONTEXT_PROVIDER_HOVER_ALPHA: u32 = 0x66; // Tauri hover:bg-theme-bg-hover/40.
const AI_CONTEXT_USER_OVERRIDE_BG_ALPHA: u32 = 0x0d; // Tauri bg-theme-accent/5.
const AI_CONTEXT_NUMBER_W: f32 = 112.0; // Tauri w-28.
const AI_CONTEXT_RESET_SLOT_W: f32 = 16.0; // Tauri w-4.
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
