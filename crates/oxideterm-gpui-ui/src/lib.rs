#![allow(dead_code)]

pub mod ai;
pub mod badge;
pub mod button;
pub mod checkbox;
pub mod command;
pub mod confirm;
pub mod context_menu;
pub mod dialog;
pub mod dropdown_menu;
pub mod font_size_hud;
pub mod form_field;
pub mod input;
pub mod label;
pub mod menu;
pub mod modal;
pub mod progress;
pub mod radio_group;
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

pub use badge::{IconBadgeMetrics, icon_badge, icon_badge_metrics_from_tokens};
pub use button::{ButtonTone, button, button_focus_visible};
pub use checkbox::{CheckboxOptions, checkbox, checkbox_with};
pub use confirm::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView, confirm_dialog,
    confirm_dialog_with_focus,
};
pub use form_field::form_field;
pub use modal::{modal_body, modal_container, modal_footer, modal_header, modal_overlay};
pub use state::{
    UiStateTone, empty_state, error_state, inline_empty_state, loading_state, state_description,
    state_icon_box, state_notice, state_primary_action, state_shell, state_title,
};
pub use surface::{
    color_for_background, color_for_background_or_alpha, color_with_alpha,
    color_with_background_scaled_alpha, scale_alpha_byte,
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
    css_font_family_head, gpui_font_family_name, tauri_cjk_ui_font_family, tauri_ui_font_family,
};
