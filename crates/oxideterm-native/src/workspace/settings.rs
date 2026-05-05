use gpui::{
    AnchoredPositionMode, Corner, Div, ObjectFit, PathPromptOptions, StatefulInteractiveElement,
    StyledImage, anchored, deferred, point,
};
use oxideterm_settings::{
    AdaptiveRendererMode, AiReasoningEffort, AiThinkingStyle, AnimationSpeed, BackgroundFit,
    ConflictAction, CursorStyle as SettingsCursorStyle, FontFamily, FrostedGlassMode,
    HighlightRule, HighlightRuleRenderMode, IdeAgentMode, Language, MAX_HIGHLIGHT_PATTERN_LENGTH,
    MAX_HIGHLIGHT_RULES, PersistedSettings, TerminalEncoding, UiDensity, UpdateChannel,
    create_default_highlight_rule, reindex_highlight_rules,
};
use oxideterm_theme::BUILT_IN_THEMES;

use super::ime::WorkspaceImeTarget;
use super::*;
use crate::ui::{
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
    text_input::{TextInputView, text_input, text_input_anchor_probe},
};

include!("settings/types.rs");

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

include!("settings/setters.rs");
include!("settings/highlight_data.rs");
include!("settings/labels.rs");
