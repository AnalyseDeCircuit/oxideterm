// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Settings page model crate.
//!
//! This crate owns non-GPUI settings page behavior: AI profile mutations,
//! provider refresh DTOs, reconnect option models, knowledge import rules,
//! plugin setting draft conversion, and compact view-model helpers.

pub mod ai;
pub mod knowledge;
pub mod plugin;
pub mod provider_models;
pub mod reconnect;

pub use ai::*;
pub use knowledge::*;
pub use plugin::*;
pub use provider_models::*;
pub use reconnect::*;
