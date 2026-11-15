use super::*;
use oxideterm_gpui_ui::IconButtonOptions;
use oxideterm_gpui_ui::button::ButtonRadius;
use oxideterm_gpui_ui::tree::{TreeGuideMetrics, tree_guide_child};

// Active sessions are a high-frequency navigator, so keep its icon scale compact
// while deriving row heights and typography from ThemeTokens.metrics.
const SESSION_TREE_ICON_SIZE: f32 = 14.0;
const SESSION_TREE_CHILD_ICON_SIZE: f32 = 12.0;
// Primary sidebar content needs a small inset below the header divider so the
// first interactive surface does not visually merge with workspace chrome.
const PRIMARY_SIDEBAR_CONTENT_TOP_INSET: f32 = 4.0;
const SESSION_ROW_SELECTED_BG_ALPHA: u32 = 0x1a;
// Tauri EventLogPanel rows use `min-h-[24px]` with `px-3 py-1`; keep the
// native estimate next to the shared virtual-list call so scroll-to-index and
// sticky-bottom behavior stay browser-like.
const EMBEDDED_SFTP_MIN_SESSION_FRACTION: f32 = 0.2;
const EMBEDDED_SFTP_MAX_SESSION_FRACTION: f32 = 0.75;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SidebarSection {
    Sessions,
    Connections,
    Forwards,
    Runtime,
    Terminal,
    Network,
    Extensions,
    CloudSync,
    Knowledge,
    Assistant,
    HostTools,
    Automation,
    Workspace,
    Files,
    Notifications,
    Settings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ContextSidebarPanel {
    Assistant,
    HostTools,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ContextSidebarTool {
    Monitor,
    Gpu,
    Processes,
    Services,
    Logs,
    Tmux,
    Docker,
    Ports,
    Schedules,
    Filesystems,
    Packages,
}

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

impl SidebarSection {
    pub(super) fn from_settings_key(key: &str) -> Self {
        match key {
            // The retired saved-connections sidebar now restores the active
            // session navigator while the full manager remains a workspace tab.
            "connections" | "saved" => Self::Sessions,
            // Embedded SFTP now shares the active-sessions panel. Preserve the
            // old persisted key as a migration path instead of reopening a
            // retired standalone sidebar.
            "sftp" => Self::Sessions,
            "forwards" => Self::Forwards,
            "runtime" => Self::Runtime,
            "connection_pool" | "terminal" => Self::Terminal,
            // Retired health-page keys now restore the Host Tools replacement.
            "connection_monitor" | "activity" => Self::HostTools,
            "network" | "topology" => Self::Network,
            "extensions" => Self::Extensions,
            "cloud_sync" => Self::CloudSync,
            "knowledge" => Self::Knowledge,
            "ai" | "assistant" => Self::Assistant,
            "host_tools" => Self::HostTools,
            "automation" => Self::Automation,
            "workspace" => Self::Workspace,
            "files" => Self::Files,
            // The standalone monitor activity button was retired after Host Tools
            // became the cross-platform owner of connection monitoring.
            "monitor" => Self::HostTools,
            "notifications" => Self::Notifications,
            "settings" => Self::Settings,
            _ => Self::Sessions,
        }
    }

    pub(super) fn as_settings_key(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            // Keep the activity identity stable; persistence stores the
            // effective Sessions panel instead of this tab-only entry.
            Self::Connections => "saved",
            Self::Forwards => "forwards",
            Self::Runtime => "runtime",
            Self::Terminal => "connection_pool",
            Self::Network => "topology",
            Self::Extensions => "extensions",
            Self::CloudSync => "cloud_sync",
            Self::Knowledge => "knowledge",
            Self::Assistant => "ai",
            Self::HostTools => "host_tools",
            Self::Automation => "automation",
            Self::Workspace => "workspace",
            Self::Files => "files",
            Self::Notifications => "notifications",
            Self::Settings => "settings",
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn effective_sidebar_panel_section(&self) -> SidebarSection {
        match self.active_sidebar_section {
            SidebarSection::Sessions
            | SidebarSection::Forwards
            | SidebarSection::Extensions
            | SidebarSection::CloudSync => self.active_sidebar_section,
            // Tauri separates activity-bar tab buttons from sidebar sections.
            // Keep tab-only entries from replacing the Sessions sidebar body.
            SidebarSection::Connections
            | SidebarSection::Terminal
            | SidebarSection::Runtime
            | SidebarSection::Network
            | SidebarSection::Knowledge
            | SidebarSection::Assistant
            | SidebarSection::HostTools
            | SidebarSection::Automation
            | SidebarSection::Workspace
            | SidebarSection::Files
            | SidebarSection::Notifications
            | SidebarSection::Settings => SidebarSection::Sessions,
        }
    }
}

mod activity;
mod ai;
mod helpers;
mod region;
pub(in crate::workspace) use region::sidebar_resize_hotzone_chrome;
mod sessions;
mod state;
mod titlebar;
mod tools;
pub(in crate::workspace) use titlebar::{
    client_titlebar_button_layout, handle_window_drag_mouse_down,
};

pub(in crate::workspace) use ai::{
    AcpApplicationToolTurn, AiChatPromptKeyAction, AiCompactionDelivery,
    AiCompactionDeliverySender, AiInlinePanelState, AiInputVisualLine, AiStreamDelivery,
    AiStreamDeliveryEvent, AiStreamDeliverySender, ai_chat_prompt_key_action,
    ai_input_line_index_for_offset, ai_input_visual_lines, ai_now_ms,
    handle_acp_application_tool_call,
};
use helpers::*;
pub(in crate::workspace) use state::{
    clamp_responsive_sidebar_width, context_sidebar_panel_visible,
};

#[cfg(test)]
mod sidebar_persistence_tests {
    use super::SidebarSection;

    #[test]
    fn sidebar_sections_roundtrip_persisted_settings_keys() {
        let sections = [
            SidebarSection::Sessions,
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
