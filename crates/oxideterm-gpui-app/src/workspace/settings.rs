use gpui::{
    AnchoredPositionMode, Corner, Div, ObjectFit, PathPromptOptions, StatefulInteractiveElement,
    StyledImage, anchored, deferred, point,
};
use oxideterm_settings::{
    FrostedGlassMode, HighlightRule, IdeAgentMode, Language, MAX_HIGHLIGHT_RULES,
    PersistedSettings, create_default_highlight_rule, reindex_highlight_rules,
};
use oxideterm_theme::BUILT_IN_THEMES;

use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_connections::{
    SshConfigHost, list_available_ssh_keys, list_ssh_config_hosts, resolve_ssh_config_alias,
    saved_connection_from_ssh_host,
};
use oxideterm_gpui_settings_view::*;
use oxideterm_gpui_ui::{
    button,
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with},
    checkbox::checkbox,
    select::{
        OverlayAnchor, SelectAnchorId, select_anchor_probe, select_label, select_option,
        select_overlay_popup, select_panel_overlay_popup_with_max_height, select_separator,
        select_trigger,
    },
    separator::{SeparatorOrientation, separator},
    slider::{SliderView, slider},
    text_input::{TextInputView, text_caret, text_input, text_input_anchor_probe},
};

include!("settings/surface.rs");
include!("settings/cards.rs");
include!("settings/controls.rs");
include!("settings/terminal_display.rs");
include!("settings/highlight.rs");
include!("settings/terminal_controls.rs");
include!("settings/local_terminal.rs");
include!("settings/general_terminal_pages.rs");
include!("settings/appearance.rs");
include!("settings/pages.rs");

fn settings_tab_lucide(icon: SettingsTabIcon) -> LucideIcon {
    match icon {
        SettingsTabIcon::BookOpen => LucideIcon::BookOpen,
        SettingsTabIcon::Code2 => LucideIcon::Code2,
        SettingsTabIcon::HardDrive => LucideIcon::HardDrive,
        SettingsTabIcon::HelpCircle => LucideIcon::HelpCircle,
        SettingsTabIcon::Key => LucideIcon::Key,
        SettingsTabIcon::Keyboard => LucideIcon::Keyboard,
        SettingsTabIcon::Monitor => LucideIcon::Monitor,
        SettingsTabIcon::Shield => LucideIcon::Shield,
        SettingsTabIcon::Sparkles => LucideIcon::Sparkles,
        SettingsTabIcon::Square => LucideIcon::Square,
        SettingsTabIcon::Terminal => LucideIcon::Terminal,
        SettingsTabIcon::WifiOff => LucideIcon::WifiOff,
    }
}

fn settings_background_tab_lucide(icon: SettingsBackgroundTabIcon) -> LucideIcon {
    match icon {
        SettingsBackgroundTabIcon::Activity => LucideIcon::Activity,
        SettingsBackgroundTabIcon::ArrowLeftRight => LucideIcon::ArrowLeftRight,
        SettingsBackgroundTabIcon::Code2 => LucideIcon::Code2,
        SettingsBackgroundTabIcon::Folder => LucideIcon::Folder,
        SettingsBackgroundTabIcon::FolderInput => LucideIcon::FolderInput,
        SettingsBackgroundTabIcon::ListTree => LucideIcon::ListTree,
        SettingsBackgroundTabIcon::Monitor => LucideIcon::Monitor,
        SettingsBackgroundTabIcon::Network => LucideIcon::Network,
        SettingsBackgroundTabIcon::Puzzle => LucideIcon::Puzzle,
        SettingsBackgroundTabIcon::Rocket => LucideIcon::Rocket,
        SettingsBackgroundTabIcon::Settings => LucideIcon::Settings,
        SettingsBackgroundTabIcon::Terminal => LucideIcon::Terminal,
    }
}
