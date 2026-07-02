// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI presentation boundary for remote desktop sessions.
//!
//! The app crate owns window routing and helper process lifetimes. This crate
//! owns only reusable view state and presentational elements so RDP and VNC can
//! share the same terminal-adjacent chrome.

mod input;
mod state;
mod view;

pub use input::{
    RemoteDesktopMappedPoint, RemoteDesktopViewportMapper, SharedRemoteDesktopGeometry,
};
pub use state::{
    RemoteDesktopCursorState, RemoteDesktopFrameApplyStats, RemoteDesktopViewSnapshot,
    RemoteDesktopViewState,
};
pub use view::{remote_desktop_surface, remote_desktop_surface_with_geometry};
