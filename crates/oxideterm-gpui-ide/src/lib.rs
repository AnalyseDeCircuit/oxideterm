// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI owner surface for OxideTerm's native IDE path.
//!
//! This crate is the first real UI owner for `oxideterm-ide-core`: it owns the
//! project tree, opened editor tabs, save dispatch, and reconnect snapshot
//! restore surface. It is deliberately transport-agnostic except for the
//! node-first IDE file-system adapter passed in from the app layer.

mod file_icons;
mod labels;
mod surface;

pub use labels::IdeLabels;
pub use oxideterm_ide_fs::NodeAgentMode;
pub use surface::{
    IdeAiContextSnapshot, IdeLoadState, IdePluginFileSnapshot, IdePluginProjectSnapshot,
    IdePluginSnapshot, IdeRuntimeSettings, IdeSurface, IdeSurfaceEvent,
};
