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
        id: "activity",
        icon: LucideIcon::Activity,
    },
    SessionIconChoice {
        id: "app-window",
        icon: LucideIcon::AppWindow,
    },
    SessionIconChoice {
        id: "arrow-left-right",
        icon: LucideIcon::ArrowLeftRight,
    },
    SessionIconChoice {
        id: "arrow-up-down",
        icon: LucideIcon::ArrowUpDown,
    },
    SessionIconChoice {
        id: "book-open",
        icon: LucideIcon::BookOpen,
    },
    SessionIconChoice {
        id: "bot",
        icon: LucideIcon::Bot,
    },
    SessionIconChoice {
        id: "brain",
        icon: LucideIcon::Brain,
    },
    SessionIconChoice {
        id: "cable",
        icon: LucideIcon::Cable,
    },
    SessionIconChoice {
        id: "check-circle",
        icon: LucideIcon::CheckCircle,
    },
    SessionIconChoice {
        id: "circle",
        icon: LucideIcon::Circle,
    },
    SessionIconChoice {
        id: "clock",
        icon: LucideIcon::Clock,
    },
    SessionIconChoice {
        id: "cloud",
        icon: LucideIcon::Cloud,
    },
    SessionIconChoice {
        id: "code",
        icon: LucideIcon::Code2,
    },
    SessionIconChoice {
        id: "cpu",
        icon: LucideIcon::Cpu,
    },
    SessionIconChoice {
        id: "download",
        icon: LucideIcon::Download,
    },
    SessionIconChoice {
        id: "file",
        icon: LucideIcon::File,
    },
    SessionIconChoice {
        id: "file-archive",
        icon: LucideIcon::FileArchive,
    },
    SessionIconChoice {
        id: "file-audio",
        icon: LucideIcon::FileAudio,
    },
    SessionIconChoice {
        id: "file-code",
        icon: LucideIcon::FileCode,
    },
    SessionIconChoice {
        id: "file-image",
        icon: LucideIcon::FileImage,
    },
    SessionIconChoice {
        id: "file-json",
        icon: LucideIcon::FileJson,
    },
    SessionIconChoice {
        id: "file-lock",
        icon: LucideIcon::FileLock,
    },
    SessionIconChoice {
        id: "file-play",
        icon: LucideIcon::FilePlay,
    },
    SessionIconChoice {
        id: "file-plus",
        icon: LucideIcon::FilePlus,
    },
    SessionIconChoice {
        id: "file-spreadsheet",
        icon: LucideIcon::FileSpreadsheet,
    },
    SessionIconChoice {
        id: "file-terminal",
        icon: LucideIcon::FileTerminal,
    },
    SessionIconChoice {
        id: "file-text",
        icon: LucideIcon::FileText,
    },
    SessionIconChoice {
        id: "file-video",
        icon: LucideIcon::FileVideo,
    },
    SessionIconChoice {
        id: "folder",
        icon: LucideIcon::Folder,
    },
    SessionIconChoice {
        id: "folder-archive",
        icon: LucideIcon::FolderArchive,
    },
    SessionIconChoice {
        id: "folder-input",
        icon: LucideIcon::FolderInput,
    },
    SessionIconChoice {
        id: "folder-open",
        icon: LucideIcon::FolderOpen,
    },
    SessionIconChoice {
        id: "folder-plus",
        icon: LucideIcon::FolderPlus,
    },
    SessionIconChoice {
        id: "folder-sync",
        icon: LucideIcon::FolderSync,
    },
    SessionIconChoice {
        id: "gauge",
        icon: LucideIcon::Gauge,
    },
    SessionIconChoice {
        id: "git-fork",
        icon: LucideIcon::GitFork,
    },
    SessionIconChoice {
        id: "hard-drive",
        icon: LucideIcon::HardDrive,
    },
    SessionIconChoice {
        id: "hash",
        icon: LucideIcon::Hash,
    },
    SessionIconChoice {
        id: "history",
        icon: LucideIcon::History,
    },
    SessionIconChoice {
        id: "home",
        icon: LucideIcon::Home,
    },
    SessionIconChoice {
        id: "image",
        icon: LucideIcon::Image,
    },
    SessionIconChoice {
        id: "inbox",
        icon: LucideIcon::Inbox,
    },
    SessionIconChoice {
        id: "key",
        icon: LucideIcon::Key,
    },
    SessionIconChoice {
        id: "key-round",
        icon: LucideIcon::KeyRound,
    },
    SessionIconChoice {
        id: "keyboard",
        icon: LucideIcon::Keyboard,
    },
    SessionIconChoice {
        id: "layers",
        icon: LucideIcon::Layers,
    },
    SessionIconChoice {
        id: "layout-list",
        icon: LucideIcon::LayoutList,
    },
    SessionIconChoice {
        id: "link",
        icon: LucideIcon::Link2,
    },
    SessionIconChoice {
        id: "list-checks",
        icon: LucideIcon::ListChecks,
    },
    SessionIconChoice {
        id: "list-tree",
        icon: LucideIcon::ListTree,
    },
    SessionIconChoice {
        id: "lock",
        icon: LucideIcon::Lock,
    },
    SessionIconChoice {
        id: "memory-stick",
        icon: LucideIcon::MemoryStick,
    },
    SessionIconChoice {
        id: "message-square",
        icon: LucideIcon::MessageSquare,
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
        id: "pin",
        icon: LucideIcon::Pin,
    },
    SessionIconChoice {
        id: "power",
        icon: LucideIcon::Power,
    },
    SessionIconChoice {
        id: "puzzle",
        icon: LucideIcon::Puzzle,
    },
    SessionIconChoice {
        id: "radio",
        icon: LucideIcon::Radio,
    },
    SessionIconChoice {
        id: "refresh",
        icon: LucideIcon::RefreshCw,
    },
    SessionIconChoice {
        id: "rocket",
        icon: LucideIcon::Rocket,
    },
    SessionIconChoice {
        id: "save",
        icon: LucideIcon::Save,
    },
    SessionIconChoice {
        id: "search",
        icon: LucideIcon::Search,
    },
    SessionIconChoice {
        id: "server",
        icon: LucideIcon::Server,
    },
    SessionIconChoice {
        id: "shield",
        icon: LucideIcon::Shield,
    },
    SessionIconChoice {
        id: "shield-alert",
        icon: LucideIcon::ShieldAlert,
    },
    SessionIconChoice {
        id: "shield-check",
        icon: LucideIcon::ShieldCheck,
    },
    SessionIconChoice {
        id: "shield-off",
        icon: LucideIcon::ShieldOff,
    },
    SessionIconChoice {
        id: "sparkles",
        icon: LucideIcon::Sparkles,
    },
    SessionIconChoice {
        id: "split-horizontal",
        icon: LucideIcon::SplitSquareHorizontal,
    },
    SessionIconChoice {
        id: "split-vertical",
        icon: LucideIcon::SplitSquareVertical,
    },
    SessionIconChoice {
        id: "star",
        icon: LucideIcon::Star,
    },
    SessionIconChoice {
        id: "stop-circle",
        icon: LucideIcon::StopCircle,
    },
    SessionIconChoice {
        id: "terminal",
        icon: LucideIcon::Terminal,
    },
    SessionIconChoice {
        id: "upload",
        icon: LucideIcon::Upload,
    },
    SessionIconChoice {
        id: "wifi",
        icon: LucideIcon::Wifi,
    },
    SessionIconChoice {
        id: "wifi-off",
        icon: LucideIcon::WifiOff,
    },
    SessionIconChoice {
        id: "wrench",
        icon: LucideIcon::Wrench,
    },
    SessionIconChoice {
        id: "zap",
        icon: LucideIcon::Zap,
    },
];

pub(super) fn session_icon_from_id(icon_id: Option<&str>) -> Option<LucideIcon> {
    let icon_id = icon_id?.trim();
    let legacy_icon = match icon_id {
        "database" => Some(LucideIcon::Archive),
        "docker" => Some(LucideIcon::Layers),
        "kubernetes" => Some(LucideIcon::Puzzle),
        _ => None,
    };
    if legacy_icon.is_some() {
        return legacy_icon;
    }
    SESSION_ICON_CHOICES
        .iter()
        .find(|choice| choice.id == icon_id)
        .map(|choice| choice.icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_exposes_broad_generic_icon_set() {
        assert!(SESSION_ICON_CHOICES.len() >= 70);
    }

    #[test]
    fn legacy_semantic_icon_ids_remain_supported() {
        assert!(matches!(
            session_icon_from_id(Some("database")),
            Some(LucideIcon::Archive)
        ));
        assert!(matches!(
            session_icon_from_id(Some("docker")),
            Some(LucideIcon::Layers)
        ));
        assert!(matches!(
            session_icon_from_id(Some("kubernetes")),
            Some(LucideIcon::Puzzle)
        ));
    }
}
