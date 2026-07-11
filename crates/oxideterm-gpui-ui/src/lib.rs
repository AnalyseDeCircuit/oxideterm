#![allow(dead_code)]

pub mod action_row;
pub mod ai;
pub mod badge;
pub mod button;
pub mod checkbox;
pub mod command;
pub mod command_panel;
pub mod confirm;
pub mod context_menu;
pub mod dialog;
pub mod dropdown_menu;
pub mod entity_row;
pub mod font_size_hud;
pub mod form_field;
pub mod input;
pub mod label;
pub mod menu;
pub mod modal;
pub mod motion;
pub mod progress;
pub mod radio_group;
pub mod scroll;
pub mod section;
pub mod select;
pub mod separator;
pub mod slider;
pub mod state;
pub mod surface;
pub mod table;
pub mod tabs;
pub mod text_input;
pub mod toast;
pub mod toaster;
pub mod tooltip;
pub mod tree;
pub mod typography;

pub use action_row::{ActionSlotRowAlignment, ActionSlotRowOptions, action_slot_row};
pub use badge::{
    IconBadgeMetrics, StatusPillColors, StatusPillOptions, StatusPillSize, StatusTone, icon_badge,
    icon_badge_metrics_from_tokens, status_pill, status_pill_colors, status_pill_element,
};
pub use button::{
    ActionChipOptions, ActionChipTextTone, ButtonTone, ContextChipOptions, IconButtonOptions,
    SplitFooterButtonOptions, ToolbarButtonIconPosition, ToolbarButtonOptions, action_chip,
    action_chip_foreground, button, button_focus_visible, context_chip, icon_button,
    split_footer_button, tauri_focus_visible_ring, toolbar_button,
};
pub use checkbox::{CheckboxOptions, checkbox, checkbox_with};
pub use command_panel::{CommandPanelOptions, command_panel, command_panel_body};
pub use confirm::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView, confirm_dialog,
    confirm_dialog_with_focus,
};
pub use entity_row::{EntityListRowDensity, EntityListRowOptions, entity_list_row};
pub use form_field::form_field;
pub use modal::{modal_body, modal_container, modal_footer, modal_header, modal_overlay};
pub use scroll::{
    ScrollViewportContract, ScrollViewportKind, Scrollable, ScrollableElement, Scrollbar,
    ScrollbarAxis,
};
pub use section::{SectionHeaderOptions, section_header};
pub use state::{
    UiStateTone, empty_state, error_state, inline_empty_state, loading_state, state_description,
    state_icon_box, state_notice, state_primary_action, state_shell, state_title,
};
pub use surface::{
    SurfaceChrome, SurfaceKind, SurfaceOptions, SurfacePadding, color_for_background,
    color_for_background_or_alpha, color_with_alpha, color_with_background_scaled_alpha,
    scale_alpha_byte, semantic_surface, surface_chrome, surface_padding_px, tauri_card_shadow,
    tauri_card_surface, tauri_glass_surface_shadow, theme_card_shadow, theme_card_surface,
    theme_card_surface_shadow, theme_overlay_shadow, theme_overlay_surface_shadow,
};
pub use table::{
    TauriTableCellOptions, TauriTableCellStyle, TauriTableColors, TauriTableMetrics,
    tauri_table_cell, tauri_table_checkbox_cell, tauri_table_header, tauri_table_row,
    tauri_table_sort_header, tauri_table_spacer_cell,
};
pub use tabs::{segmented_tab, segmented_tabs};
pub use text_input::{TextInputView, text_input, text_input_anchor_probe};
pub use tree::{TreeBranchMetrics, tree_child};
pub use typography::{
    MonospaceDatumOptions, MonospaceDatumTone, css_font_family_head, gpui_font_family_name,
    middle_truncate_text, monospace_datum, monospace_datum_color, tauri_cjk_ui_font_family,
    tauri_ui_font_family,
};
