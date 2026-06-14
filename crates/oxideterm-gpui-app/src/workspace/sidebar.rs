use super::session_manager::{
    SAVED_CONNECTION_VIRTUAL_OVERSCAN, SAVED_CONNECTION_VIRTUAL_ROW_HEIGHT, SessionManagerInput,
};
use super::*;
use oxideterm_gpui_ui::{TreeBranchMetrics, tree_child};

const SESSION_TREE_NODE_HEIGHT: f32 = 32.0;
const SESSION_TREE_ITEM_HEIGHT: f32 = 28.0;
const SESSION_TREE_TEXT_SIZE: f32 = 13.0;
const SESSION_TREE_META_TEXT_SIZE: f32 = 11.0;
const SESSION_TREE_ICON_SIZE: f32 = 16.0;
const SESSION_TREE_CHILD_ICON_SIZE: f32 = 14.0;
// Tauri FocusedNodeList uses accent/emerald alpha utility classes such as
// `bg-oxide-accent/5`, `border-oxide-accent/50`, and `bg-emerald-500/20`.
// Keep the translated alpha roles named so this card view does not drift into
// feature-local magic colors.
const SESSION_FOCUS_CARD_SELECTED_BG_ALPHA: u32 = 0x0d;
const SESSION_FOCUS_CARD_SELECTED_BORDER_ALPHA: u32 = 0x80;
const SESSION_FOCUS_CARD_BORDER_ALPHA: u32 = 0x80;
const SESSION_FOCUS_TERMINAL_ACTIVE_BG_ALPHA: u32 = 0x1a;
const SESSION_FOCUS_TERMINAL_BADGE_BG_ALPHA: u32 = 0x33;
const SESSION_FOCUS_TERMINAL_BADGE_HOVER_ALPHA: u32 = 0x4d;
const SESSION_FOCUS_ACTION_BG_ALPHA: u32 = 0x1a;
const SESSION_FOCUS_ACTION_HOVER_ALPHA: u32 = 0x26;
const SESSION_FOCUS_DIVIDER_ALPHA: u32 = 0x4d;
// Tauri FocusedNodeList empty state uses `w-8 h-8 opacity-30`,
// `text-sm`, `text-xs`, and `opacity-60` for the helper text.
const SESSION_FOCUS_EMPTY_ICON_SIZE: f32 = 32.0;
const SESSION_FOCUS_EMPTY_ICON_ALPHA: u32 = 0x4d;
const SESSION_FOCUS_EMPTY_TITLE_TEXT_SIZE: f32 = 14.0;
const SESSION_FOCUS_EMPTY_SUBTITLE_TEXT_SIZE: f32 = 12.0;
const SESSION_FOCUS_EMPTY_SUBTITLE_ALPHA: f32 = 0.6;
const SESSION_FOCUS_EMERALD: u32 = 0x10b981;
// Tauri EventLogPanel rows use `min-h-[24px]` with `px-3 py-1`; keep the
// native estimate next to the shared virtual-list call so scroll-to-index and
// sticky-bottom behavior stay browser-like.
const EVENT_LOG_SIDEBAR_ROW_HEIGHT: f32 = 24.0;
const EVENT_LOG_SIDEBAR_VIRTUAL_OVERSCAN: usize = 20;
const EVENT_LOG_STICKY_BOTTOM_THRESHOLD_PX: f32 = 30.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SidebarSection {
    Sessions,
    Connections,
    Sftp,
    Forwards,
    Runtime,
    Terminal,
    Activity,
    Network,
    Extensions,
    CloudSync,
    Assistant,
    HostTools,
    Automation,
    Workspace,
    Files,
    Monitor,
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
    Processes,
    Services,
    Logs,
    Tmux,
    Docker,
}

#[derive(Clone, Copy)]
struct SessionStatusStyle {
    icon: LucideIcon,
    text_color: u32,
    dot_color: u32,
    opacity: f32,
    ring: bool,
}

#[derive(Clone, Copy)]
enum SessionActionVariant {
    Primary,
    Danger,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ActiveSessionSidebarViewMode {
    Tree,
    Focus,
}

impl SidebarSection {
    pub(super) fn from_settings_key(key: &str) -> Self {
        match key {
            "connections" | "saved" => Self::Connections,
            "sftp" => Self::Sftp,
            "forwards" => Self::Forwards,
            "runtime" => Self::Runtime,
            "connection_pool" | "terminal" => Self::Terminal,
            "connection_monitor" => Self::Activity,
            "activity" => Self::Activity,
            "network" | "topology" => Self::Network,
            "extensions" => Self::Extensions,
            "cloud_sync" => Self::CloudSync,
            "ai" | "assistant" => Self::Assistant,
            "host_tools" => Self::HostTools,
            "automation" => Self::Automation,
            "workspace" => Self::Workspace,
            "files" => Self::Files,
            "monitor" => Self::Monitor,
            "notifications" => Self::Notifications,
            "settings" => Self::Settings,
            _ => Self::Sessions,
        }
    }

    pub(super) fn as_settings_key(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            // Tauri persists the saved-connections sidebar as `saved`.
            Self::Connections => "saved",
            Self::Sftp => "sftp",
            Self::Forwards => "forwards",
            Self::Runtime => "runtime",
            Self::Terminal => "connection_pool",
            Self::Activity => "activity",
            Self::Network => "topology",
            Self::Extensions => "extensions",
            Self::CloudSync => "cloud_sync",
            Self::Assistant => "ai",
            Self::HostTools => "host_tools",
            Self::Automation => "automation",
            Self::Workspace => "workspace",
            Self::Files => "files",
            Self::Monitor => "monitor",
            Self::Notifications => "notifications",
            Self::Settings => "settings",
        }
    }
}

impl WorkspaceApp {
    fn effective_sidebar_panel_section(&self) -> SidebarSection {
        match self.active_sidebar_section {
            SidebarSection::Sessions
            | SidebarSection::Connections
            | SidebarSection::Sftp
            | SidebarSection::Forwards
            | SidebarSection::Extensions
            | SidebarSection::CloudSync => self.active_sidebar_section,
            // Tauri separates activity-bar tab buttons from sidebar sections.
            // Keep tab-only entries from replacing the Sessions sidebar body.
            SidebarSection::Terminal
            | SidebarSection::Runtime
            | SidebarSection::Activity
            | SidebarSection::Network
            | SidebarSection::Assistant
            | SidebarSection::HostTools
            | SidebarSection::Automation
            | SidebarSection::Workspace
            | SidebarSection::Files
            | SidebarSection::Monitor
            | SidebarSection::Notifications
            | SidebarSection::Settings => SidebarSection::Sessions,
        }
    }
}

include!("sidebar/state.rs");
include!("sidebar/titlebar.rs");
include!("sidebar/activity.rs");
include!("sidebar/ai.rs");
include!("sidebar/region.rs");
include!("sidebar/sessions.rs");
include!("sidebar/saved.rs");
include!("sidebar/helpers.rs");

#[cfg(test)]
mod sidebar_persistence_tests {
    use super::SidebarSection;

    #[test]
    fn sidebar_sections_roundtrip_persisted_settings_keys() {
        let sections = [
            SidebarSection::Sessions,
            SidebarSection::Connections,
            SidebarSection::Sftp,
            SidebarSection::Forwards,
            SidebarSection::Runtime,
            SidebarSection::Terminal,
            SidebarSection::Activity,
            SidebarSection::Network,
            SidebarSection::Extensions,
            SidebarSection::CloudSync,
            SidebarSection::Assistant,
            SidebarSection::HostTools,
            SidebarSection::Automation,
            SidebarSection::Workspace,
            SidebarSection::Files,
            SidebarSection::Monitor,
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
    fn sidebar_section_parser_accepts_legacy_saved_connection_alias() {
        assert_eq!(
            SidebarSection::from_settings_key("connections"),
            SidebarSection::Connections
        );
    }
}
