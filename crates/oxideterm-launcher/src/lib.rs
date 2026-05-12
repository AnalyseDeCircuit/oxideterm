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
pub use model::{LauncherAppEntry, LauncherListResponse, LauncherLoadResponse, WslDistro};
pub use platform::{launch_app, launch_wsl, list_apps, load_entries};
pub use query::{count_label, filter_apps, filter_wsl_distros};
pub use state::LauncherRuntimeState;
