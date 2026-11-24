use super::*;
use oxideterm_gpui_ui::IconButtonOptions;
use oxideterm_gpui_ui::button::ButtonRadius;
use oxideterm_gpui_ui::tree::{TreeGuideMetrics, tree_guide_child};

// Active sessions are a high-frequency navigator, so keep its icon scale compact
// while deriving row heights and typography from ThemeTokens.metrics.
pub(super) const SESSION_TREE_ICON_SIZE: f32 = 14.0;
const SESSION_TREE_CHILD_ICON_SIZE: f32 = 12.0;
// Primary sidebar content needs a small inset below the header divider so the
// first interactive surface does not visually merge with workspace chrome.
pub(super) const PRIMARY_SIDEBAR_CONTENT_TOP_INSET: f32 = 4.0;
pub(super) const SESSION_ROW_SELECTED_BG_ALPHA: u32 = 0x1a;
pub(super) const ACTIVE_SESSION_SELECTION_MENU_WIDTH: f32 = 220.0;
pub(super) const ACTIVE_SESSION_SELECTION_MENU_HEIGHT: f32 = 36.0;
// Tauri EventLogPanel rows use `min-h-[24px]` with `px-3 py-1`; keep the
// native estimate next to the shared virtual-list call so scroll-to-index and
// sticky-bottom behavior stay browser-like.
const EMBEDDED_SFTP_MIN_SESSION_FRACTION: f32 = 0.2;
const EMBEDDED_SFTP_MAX_SESSION_FRACTION: f32 = 0.75;
use oxideterm_gpui_shell::chrome::{context_rail_icon_button_options, context_rail_icon_color_hex};

pub(super) use oxideterm_gpui_host_tools::ContextSidebarTool;
pub(super) use oxideterm_gpui_shell::{ContextSidebarPanel, SidebarSection};

#[derive(Clone, Copy)]
pub(in crate::workspace) struct SessionStatusStyle {
    icon: LucideIcon,
    dot_color: u32,
}

#[derive(Clone, Copy)]
pub(in crate::workspace) enum SessionActionVariant {
    Primary,
    Danger,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ActiveSessionSidebarViewMode {
    Tree,
    Host,
}

impl WorkspaceApp {
    pub(in crate::workspace) fn effective_sidebar_panel_section(&self) -> SidebarSection {
        self.rails.section.panel()
    }
}

mod activity;
mod ai;
mod ai_header;
mod ai_model_picker;
mod ai_transcript;
mod helpers;
mod region;
mod sessions;
pub(in crate::workspace) use sessions::{
    ActiveSessionRemovalOutcome, ActiveSessionSelectionTarget,
};
mod state;
mod titlebar;
mod tools;
pub(in crate::workspace) use titlebar::{
    client_titlebar_button_layout, handle_window_drag_mouse_down,
};

pub(in crate::workspace) use ai::{
    AiChatPromptKeyAction, AiInputVisualLine, ai_chat_prompt_key_action,
    ai_input_line_index_for_offset, ai_input_visual_lines,
};
use helpers::*;
pub(in crate::workspace) use oxideterm_gpui_shell::clamp_responsive_sidebar_width;

#[cfg(test)]
mod sidebar_persistence_tests {
    use super::SidebarSection;

    #[test]
    fn sidebar_sections_roundtrip_persisted_settings_keys() {
        let sections = [
            SidebarSection::Sessions,
            SidebarSection::Connections,
            SidebarSection::Forwards,
            SidebarSection::Runtime,
            SidebarSection::Terminal,
            SidebarSection::Network,
            SidebarSection::Extensions,
            SidebarSection::CloudSync,
            SidebarSection::Knowledge,
            SidebarSection::Assistant,
            SidebarSection::HostTools,
            SidebarSection::Automation,
            SidebarSection::Workspace,
            SidebarSection::Files,
            SidebarSection::Notifications,
            SidebarSection::Settings,
        ];

        for section in sections {
            assert_eq!(
                SidebarSection::from_settings_key(section.as_settings_key()),
                section
            );
        }
    }

    #[test]
    fn retired_sidebar_keys_restore_active_sessions() {
        for key in ["connections", "saved", "sftp"] {
            assert_eq!(
                SidebarSection::from_settings_key(key),
                SidebarSection::Sessions
            );
        }
    }

    #[test]
    fn retired_monitor_key_restores_host_tools() {
        assert_eq!(
            SidebarSection::from_settings_key("monitor"),
            SidebarSection::HostTools
        );
    }
}
