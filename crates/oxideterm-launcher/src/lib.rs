// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Platform application launcher core.
//!
//! This crate mirrors the Tauri launcher backend and keeps launcher state
//! transitions out of GPUI view code. The native UI should render this model,
//! not own a separate scan/cache/filter lifecycle.

mod cache;
mod model;
mod platform;
mod query;
mod state;

pub use cache::{clear_icon_cache, icon_cache_dir};
pub use model::{LauncherAppEntry, LauncherListResponse};
pub use platform::{launch_app, list_apps};
pub use query::{count_label, filter_apps};
pub use state::LauncherRuntimeState;
