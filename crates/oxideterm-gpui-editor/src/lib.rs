// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI text editor surface for OxideTerm's native IDE path.
//!
//! This crate deliberately starts with virtualized plain-text rendering only.
//! Syntax, project ownership, and remote save semantics belong to later crates
//! in `docs/native-editor-ide-plan.md`.

mod metrics;
mod settings;
mod surface;
mod viewport;

pub use metrics::{EditorAppearance, EditorMetrics};
pub use settings::EditorSettings;
pub use surface::{EditorCommand, EditorSaveStatus, SaveCallback, TextEditorView};
pub use viewport::{EditorViewport, VisibleRows};
