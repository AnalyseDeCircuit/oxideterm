// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI presentation boundary for remote desktop sessions.
//!
//! The app routes windows and tabs. This crate owns reusable presentation,
//! remote resize policy, and the per-session helper worker.

mod input;
pub mod resize;
mod session;
mod state;
mod view;
pub mod worker;

pub use input::{
    RemoteDesktopMappedPoint, RemoteDesktopViewportMapper, SharedRemoteDesktopGeometry,
};
pub use session::{
    DesktopCredentialPrompt, DesktopShortcutMatcher, DesktopWindowAction,
    PendingRemoteDesktopSshTunnel, RemoteDesktopCertificateAcceptance,
    RemoteDesktopCertificateChallengeState, RemoteDesktopPublicClipboardSnapshot,
    RemoteDesktopSessionEntity, RemoteDesktopSessionEvent, RemoteDesktopSshTunnelLease,
    RemoteDesktopWorkspaceEntity, desktop_connection_audit, desktop_shortcut_matches,
    preview_remote_desktop_profile, remote_desktop_audit_context, remote_desktop_keyboard_capture,
    remote_desktop_tab_visible, render_remote_desktop_panel,
};
pub use state::{
    RemoteDesktopCursorState, RemoteDesktopFrameApplyStats, RemoteDesktopFrameSnapshot,
    RemoteDesktopViewSnapshot, RemoteDesktopViewState,
};
pub use view::{remote_desktop_surface, remote_desktop_surface_with_geometry};
