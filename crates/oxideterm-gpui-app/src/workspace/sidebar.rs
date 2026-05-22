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
    Terminal,
    Activity,
    Network,
    Extensions,
    CloudSync,
    Assistant,
    Automation,
    Workspace,
    Files,
    Monitor,
    Notifications,
    Settings,
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

impl SidebarSection {
    pub(super) fn from_settings_key(key: &str) -> Self {
        match key {
            "connections" | "saved" => Self::Connections,
            "connection_pool" | "sftp" | "terminal" => Self::Terminal,
            "connection_monitor" => Self::Activity,
            "forwards" | "activity" => Self::Activity,
            "network" | "topology" => Self::Network,
            "extensions" => Self::Extensions,
            "cloud_sync" => Self::Sessions,
            "ai" | "assistant" => Self::Sessions,
            "automation" => Self::Automation,
            "workspace" => Self::Workspace,
            "files" => Self::Files,
            "monitor" => Self::Monitor,
            "notifications" => Self::Sessions,
            "settings" => Self::Settings,
            _ => Self::Sessions,
        }
    }

    pub(super) fn as_settings_key(self) -> &'static str {
        match self {
            Self::Sessions => "sessions",
            Self::Connections => "connections",
            Self::Terminal => "connection_pool",
            Self::Activity => "activity",
            Self::Network => "topology",
            Self::Extensions => "extensions",
            Self::CloudSync => "cloud_sync",
            Self::Assistant => "ai",
            Self::Automation => "automation",
            Self::Workspace => "workspace",
            Self::Files => "files",
            Self::Monitor => "monitor",
            Self::Notifications => "notifications",
            Self::Settings => "settings",
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
