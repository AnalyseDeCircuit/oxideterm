// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cloud Sync panel and operation lifecycle.
//!
//! The panel owns its rendering, operation mailbox, timers, form, and previews.
//! The workspace owns window integration and applies cross-feature effects.

pub mod config;
pub mod delivery;
pub mod entity;
pub mod form;
pub mod format;
pub mod guide;
pub mod labels;
pub mod panel;
pub mod preview;
pub mod runtime;
pub mod selection;
pub mod signatures;
pub mod state_finish;
pub mod status;
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
pub use status::*;
pub use view::*;
pub use view_state::*;
