// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI owner surface for OxideTerm's native IDE path.
//!
//! This crate is the first real UI owner for `oxideterm-ide-core`: it owns the
//! project tree, opened editor tabs, save dispatch, and reconnect snapshot
//! restore surface. It is deliberately transport-agnostic except for the
//! `NodeSftpIdeFileSystem` adapter passed in from the app layer.

mod labels;
mod surface;

pub use labels::IdeLabels;
pub use surface::{IdeLoadState, IdeSurface};
