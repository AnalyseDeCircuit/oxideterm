// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI owner surface for OxideTerm's IDE workspace.
//!
//! The project tree, editor tabs, Markdown preview, and save handling belong
//! to one surface. Its file provider is node-backed on desktop and supplied by
//! the embedding application on the web.

mod labels;
#[cfg(target_family = "wasm")]
pub mod portable;
mod surface;

pub use labels::IdeLabels;
pub use oxideterm_ide_core::{IdePluginFileSnapshot, IdePluginProjectSnapshot, IdePluginSnapshot};
#[cfg(not(target_family = "wasm"))]
pub use oxideterm_ide_fs::NodeAgentMode;
pub use surface::{
    IdeAiContextSnapshot, IdeLoadState, IdeModalLayer, IdeRuntimeSettings, IdeSurface,
    IdeSurfaceEvent, IdeSurfaceMount,
};
