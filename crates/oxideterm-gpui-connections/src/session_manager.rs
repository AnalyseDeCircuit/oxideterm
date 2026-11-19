// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod batch_actions;
mod batch_state;
mod batch_templates;
mod catalog;
pub(crate) mod catalog_view;
mod groups;
mod interaction;
mod labels;
mod mutations;
mod oxide_assets;
mod oxide_delivery;
pub(crate) mod oxide_dialogs;
mod oxide_keys;
mod oxide_models;
mod oxide_transfer;
mod state;

pub use batch_state::*;
pub use batch_templates::*;
pub use catalog::*;
pub use catalog_view::{CatalogAction, CatalogEnvironment};
pub use groups::*;
pub use interaction::*;
pub use labels::*;
pub use mutations::{
    CatalogDeletion, CatalogRemoval, SavedConsoleProfileKind, delete_catalog_selection,
    remove_catalog_target,
};
pub use oxide_assets::*;
pub use oxide_delivery::*;
pub use oxide_dialogs::{
    BatchWindowAction, ManagerTextRole, OxideDialogAction, OxideDialogEnvironment,
    SessionManagerViewBindings,
};
pub use oxide_keys::OxideKeyScope;
pub use oxide_models::*;
pub use oxide_transfer::OxideTransfer;
pub use state::*;

pub const BG_ACTIVE_THEME_ALPHA: u32 = 0x99; // Nested fills share the tab's sidebar-bounded tint budget.
pub const BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // Tauri bg-hover 50%
pub const BG_ACTIVE_ROW_HOVER_ALPHA: u32 = 0x4d; // Keep full-width row hover quieter than compact controls.
pub const ROW_HOVER_ALPHA: u32 = 0x66; // Plain-theme rows use the same restrained hierarchy as image-backed rows.
pub const BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri border 75%
pub const BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60; // Tauri border/50 after active border mix
pub const MANAGER_ROW_TEXT_SIZE: f32 = 14.0;
pub const MANAGER_DRAG_PREVIEW_MAX_WIDTH: f32 = 280.0;
pub const MANAGER_DRAG_PREVIEW_RADIUS: f32 = 8.0;

pub const MANAGER_GRID_ESTIMATED_ROW_HEIGHT: f32 = 84.0;
pub const MANAGER_LIST_ESTIMATED_ROW_HEIGHT: f32 = 57.0;
pub const MANAGER_TREE_ESTIMATED_ROW_HEIGHT: f32 = 52.0;
pub const MANAGER_MAIN_VIEW_OVERSCAN: usize = 6;
pub const OXIDE_EXPORT_CONNECTION_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub const OXIDE_EXPORT_CONNECTION_LIST_ESTIMATED_HEIGHT: f32 = 58.0;
pub const OXIDE_EXPORT_CONNECTION_LIST_OVERSCAN: usize = 8;
pub const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_ESTIMATED_HEIGHT: f32 = 22.0;
pub const OXIDE_IMPORT_CONNECTION_PREVIEW_LIST_OVERSCAN: usize = 8;
pub const OXIDE_EXPORT_FORWARD_GROUP_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub const OXIDE_EXPORT_FORWARD_GROUP_LIST_ESTIMATED_HEIGHT: f32 = 84.0;
pub const OXIDE_EXPORT_FORWARD_GROUP_LIST_OVERSCAN: usize = 4;
pub const OXIDE_EXPORT_SUMMARY_LINE_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub const OXIDE_EXPORT_SUMMARY_LINE_LIST_ESTIMATED_HEIGHT: f32 = 18.0;
pub const OXIDE_EXPORT_SUMMARY_LINE_LIST_OVERSCAN: usize = 6;
pub const OXIDE_IMPORT_FORWARD_DETAIL_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub const OXIDE_IMPORT_FORWARD_DETAIL_LIST_ESTIMATED_HEIGHT: f32 = 36.0;
pub const OXIDE_IMPORT_FORWARD_DETAIL_LIST_OVERSCAN: usize = 6;
pub const OXIDE_IMPORT_NAME_GROUP_LIST_INITIAL_ITEM_COUNT: usize = 0;
pub const OXIDE_IMPORT_NAME_GROUP_LIST_ESTIMATED_HEIGHT: f32 = 28.0;
pub const OXIDE_IMPORT_NAME_GROUP_LIST_OVERSCAN: usize = 6;

#[cfg(test)]
mod tests;
