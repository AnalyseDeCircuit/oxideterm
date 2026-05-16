use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{
    AnchoredPositionMode, Corner, Div, ObjectFit, PathPromptOptions, Rgba,
    StatefulInteractiveElement, StyledImage, anchored, deferred, point,
};
use gpui_component::scroll::ScrollableElement;
use oxideterm_settings::{
    FrostedGlassMode, HighlightRule, IdeAgentMode, Language, MAX_HIGHLIGHT_RULES,
    PersistedSettings, create_default_highlight_rule, reindex_highlight_rules,
};
use oxideterm_theme::BUILT_IN_THEMES;

use super::ime::WorkspaceImeTarget;
use super::*;
use oxideterm_ai::{
    AI_PROVIDER_TEMPLATES, AiProviderKeyDisplayState, AiProviderRefreshKeyPolicy, AiProviderView,
    ContextWindowSource, add_provider_from_template as ai_add_provider_from_template,
    apply_provider_model_refresh as ai_apply_provider_model_refresh, fetch_provider_models,
    generated_provider_id, model_context_window_info as ai_model_context_window_info,
    provider_id as ai_provider_id, provider_key_display_state as ai_provider_key_display_state,
    provider_refresh_key_policy as ai_provider_refresh_key_policy,
    provider_string as ai_provider_string,
    provider_template_by_type as ai_provider_template_by_type, provider_view as ai_provider_view,
    provider_views as ai_provider_views_from_values,
    remove_provider_at_with_scoped_settings as ai_remove_provider_at_with_scoped_settings,
    set_active_provider_selection as ai_set_active_provider_selection,
    set_provider_default_model as ai_set_provider_default_model,
    take_provider_key_secret as ai_take_provider_key_secret,
    update_provider as ai_update_provider_values,
};
use oxideterm_connections::{
    SshConfigHost, list_available_ssh_keys, list_ssh_config_hosts, resolve_ssh_config_alias,
    saved_connection_from_ssh_host,
};
use oxideterm_gpui_settings_view::*;
use oxideterm_gpui_ui::{
    button,
    button::{ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with},
    checkbox::checkbox,
    modal::{
        dialog_backdrop, dialog_content, dialog_description, dialog_footer, dialog_header,
        dialog_title, popover_backdrop,
    },
    select::{
        OverlayAnchor, SelectAnchorId, select_anchor_probe, select_label, select_option,
        select_overlay_popup, select_panel_overlay_popup_with_max_height, select_separator,
        select_trigger,
    },
    separator::{SeparatorOrientation, separator},
    slider::{SliderView, slider},
    text_input::{TextInputView, text_caret, text_input, text_input_anchor_probe},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ThemeEditorSection {
    Terminal,
    Ui,
}

#[derive(Clone, Debug)]
pub(super) struct ThemeEditorState {
    pub(super) edit_theme_id: Option<String>,
    pub(super) name: String,
    pub(super) duplicate_theme: String,
    pub(super) duplicate_theme_touched: bool,
    pub(super) terminal_colors: Vec<String>,
    pub(super) ui_colors: Vec<String>,
    pub(super) active_section: ThemeEditorSection,
}

include!("settings/surface.rs");
include!("settings/cards.rs");
include!("settings/controls.rs");
include!("settings/terminal_display.rs");
include!("settings/highlight.rs");
include!("settings/terminal_controls.rs");
include!("settings/local_terminal.rs");
include!("settings/general_terminal_pages.rs");
include!("settings/appearance.rs");
include!("settings/connections_page.rs");
include!("settings/sftp_page.rs");
include!("settings/ide_page.rs");
include!("settings/ai_page.rs");
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
