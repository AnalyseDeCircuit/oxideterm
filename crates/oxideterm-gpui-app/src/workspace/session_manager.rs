use std::collections::HashSet;

#[cfg(test)]
use chrono::Utc;
use gpui::prelude::*;
use oxideterm_connections::oxide_file::ImportResultEnvelope;
use oxideterm_forwarding::OwnedForwardImportRecord;

use super::*;
use crate::workspace::quick_commands::QuickCommandImportStrategy;

pub(in crate::workspace) use oxideterm_gpui_connections::configuration::form_from_saved_connection;
pub(in crate::workspace) use oxideterm_gpui_connections::session_manager::*;

mod actions;
mod batch_add_dialogs;
mod batch_add_templates;
mod ble_actions;
mod catalog_bridge;
mod oxide_actions;
mod sidebar;
mod surface;
mod views;

#[cfg(test)]
pub(in crate::workspace) mod tests;
