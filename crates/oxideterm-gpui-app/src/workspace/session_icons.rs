use crate::assets::LucideIcon;

#[derive(Clone, Copy)]
pub(super) struct SessionIconChoice {
    pub id: &'static str,
    pub icon: LucideIcon,
}

// Persist stable string ids instead of enum names so stored connections can
// survive icon rendering changes and later brand-icon additions.
pub(super) const SESSION_ICON_CHOICES: &[SessionIconChoice] = &[
    SessionIconChoice {
        id: "server",
        icon: LucideIcon::Server,
    },
    SessionIconChoice {
        id: "terminal",
        icon: LucideIcon::Terminal,
    },
    SessionIconChoice {
        id: "monitor",
        icon: LucideIcon::Monitor,
    },
    SessionIconChoice {
        id: "network",
        icon: LucideIcon::Network,
    },
    SessionIconChoice {
        id: "cloud",
        icon: LucideIcon::Cloud,
    },
    SessionIconChoice {
        id: "hard-drive",
        icon: LucideIcon::HardDrive,
    },
    SessionIconChoice {
        id: "cpu",
        icon: LucideIcon::Cpu,
    },
    SessionIconChoice {
        id: "memory-stick",
        icon: LucideIcon::MemoryStick,
    },
    SessionIconChoice {
        id: "cable",
        icon: LucideIcon::Cable,
    },
    SessionIconChoice {
        id: "radio",
        icon: LucideIcon::Radio,
    },
    SessionIconChoice {
        id: "wifi",
        icon: LucideIcon::Wifi,
    },
    SessionIconChoice {
        id: "shield",
        icon: LucideIcon::Shield,
    },
    SessionIconChoice {
        id: "lock",
        icon: LucideIcon::Lock,
    },
    SessionIconChoice {
        id: "key",
        icon: LucideIcon::Key,
    },
    SessionIconChoice {
        id: "folder",
        icon: LucideIcon::Folder,
    },
    SessionIconChoice {
        id: "file-code",
        icon: LucideIcon::FileCode,
    },
    SessionIconChoice {
        id: "database",
        icon: LucideIcon::Archive,
    },
    SessionIconChoice {
        id: "docker",
        icon: LucideIcon::Layers,
    },
    SessionIconChoice {
        id: "kubernetes",
        icon: LucideIcon::Puzzle,
    },
    SessionIconChoice {
        id: "rocket",
        icon: LucideIcon::Rocket,
    },
    SessionIconChoice {
        id: "activity",
        icon: LucideIcon::Activity,
    },
    SessionIconChoice {
        id: "gauge",
        icon: LucideIcon::Gauge,
    },
    SessionIconChoice {
        id: "settings",
        icon: LucideIcon::Settings,
    },
    SessionIconChoice {
        id: "zap",
        icon: LucideIcon::Zap,
    },
];

pub(super) fn session_icon_from_id(icon_id: Option<&str>) -> Option<LucideIcon> {
    let icon_id = icon_id?.trim();
    SESSION_ICON_CHOICES
        .iter()
        .find(|choice| choice.id == icon_id)
        .map(|choice| choice.icon)
}
