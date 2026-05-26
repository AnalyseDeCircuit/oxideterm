// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync workspace view models and pure state transitions.
//!
//! This crate owns Cloud Sync panel domain state that does not need
//! `WorkspaceApp`: preview summaries, import selection rules, section/list
//! signatures, formatting helpers, and select focus transitions.

pub mod config;
pub mod delivery;
pub mod form;
pub mod format;
pub mod guide;
pub mod labels;
pub mod preview;
pub mod selection;
pub mod signatures;
pub mod state_finish;
pub mod view;
pub mod view_state;

pub use config::*;
pub use delivery::*;
pub use form::*;
pub use format::*;
pub use guide::*;
pub use labels::*;
pub use preview::*;
pub use selection::*;
pub use signatures::*;
pub use state_finish::*;
pub use view::*;
pub use view_state::*;
