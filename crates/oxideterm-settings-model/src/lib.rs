// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Settings page model crate.
//!
//! This crate owns non-GPUI settings page behavior: AI profile mutations,
//! provider refresh DTOs, reconnect option models, knowledge import rules,
//! plugin setting draft conversion, and compact view-model helpers.

pub mod ai;
pub mod input_draft;
pub mod knowledge;
pub mod navigation;
pub mod plugin;
pub mod provider_models;
pub mod reconnect;
pub mod state;
pub mod theme;
pub mod types;

pub use ai::*;
pub use input_draft::*;
pub use knowledge::*;
pub use navigation::*;
pub use plugin::*;
pub use provider_models::*;
pub use reconnect::*;
pub use state::*;
pub use theme::*;
pub use types::*;
