pub mod ai;
pub mod appearance;
pub mod appearance_page;
pub mod components;
pub mod dialog;
pub mod general_page;
pub mod help_page;
pub mod ide_page;
pub mod input;
pub mod keybinding_page;
pub mod keybindings;
mod known_hosts_page;
pub mod labels;
mod navigation;
pub mod options;
pub mod proxy_page;
mod search;
pub mod select_menu;
pub mod sftp_page;
pub mod slider;
pub mod ssh_import;
pub mod surface;
pub mod terminal_page;
pub mod theme_editor;
pub mod theme_picker;
pub mod types;

pub use ai::*;
pub use appearance::*;
pub use labels::*;
pub use navigation::{SettingsNavigationDraftAction, SettingsRouteSnapshot, SettingsRouteState};
pub use options::*;
pub use search::{SettingsSearchResult, settings_search_results};
pub use types::*;

#[cfg(not(target_family = "wasm"))]
pub mod app_lock_page;
#[cfg(not(target_family = "wasm"))]
pub mod background_page;
#[cfg(not(target_family = "wasm"))]
pub mod cli_companion;
#[cfg(not(target_family = "wasm"))]
pub mod command_specs;
#[cfg(not(target_family = "wasm"))]
pub mod connections_page;
#[cfg(not(target_family = "wasm"))]
pub mod credentials_page;
#[cfg(not(target_family = "wasm"))]
pub mod entity;
#[cfg(not(target_family = "wasm"))]
pub mod highlight;
#[cfg(not(target_family = "wasm"))]
pub mod highlight_page;
#[cfg(not(target_family = "wasm"))]
pub mod knowledge_page;
#[cfg(target_family = "wasm")]
pub mod knowledge_page {
    pub mod dialog;
}
#[cfg(not(target_family = "wasm"))]
pub mod local_terminal;
#[cfg(not(target_family = "wasm"))]
pub mod mcp_page;
#[cfg(not(target_family = "wasm"))]
pub mod native_update;
#[cfg(not(target_family = "wasm"))]
pub mod navigation_editor;
#[cfg(not(target_family = "wasm"))]
pub mod portable_runtime;
#[cfg(not(target_family = "wasm"))]
pub mod remote_shell;
#[cfg(not(target_family = "wasm"))]
pub mod terminal_triggers;
#[cfg(not(target_family = "wasm"))]
pub mod update_view;
#[cfg(not(target_family = "wasm"))]
pub mod views;
#[cfg(not(target_family = "wasm"))]
pub mod window_chrome;

#[cfg(not(target_family = "wasm"))]
pub use highlight::*;
