// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! WSL graphics backend primitives ported from the Tauri graphics module.
//!
//! This crate intentionally owns the Windows/WSLg command semantics so UI crates
//! can render launcher or graphics state without reimplementing `wsl.exe` parsing.

pub mod bridge;
mod error;
mod model;
pub mod session;
pub mod wsl;
pub mod wslg;

pub use error::{WSL_GRAPHICS_UNAVAILABLE, WslGraphicsError};
pub use model::{
    DesktopCandidate, GraphicsSessionMode, PrerequisiteResult, WslDistro, WslGraphicsSession,
    WslgStatus, desktop_candidates,
};
pub use session::WslGraphicsState;
