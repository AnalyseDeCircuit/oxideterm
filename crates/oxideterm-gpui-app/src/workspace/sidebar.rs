use super::session_manager::SessionManagerInput;
use super::*;
use gpui_component::scroll::ScrollableElement;
use oxideterm_gpui_ui::{TreeBranchMetrics, tree_child};

const SESSION_TREE_NODE_HEIGHT: f32 = 32.0;
const SESSION_TREE_ITEM_HEIGHT: f32 = 28.0;
const SESSION_TREE_TEXT_SIZE: f32 = 13.0;
const SESSION_TREE_META_TEXT_SIZE: f32 = 11.0;
const SESSION_TREE_ICON_SIZE: f32 = 16.0;
const SESSION_TREE_CHILD_ICON_SIZE: f32 = 14.0;

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
