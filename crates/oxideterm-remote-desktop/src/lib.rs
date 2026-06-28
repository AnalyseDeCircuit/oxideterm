// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Protocol-neutral remote desktop domain primitives.
//!
//! This crate deliberately avoids GPUI, SSH handles, and concrete RDP/VNC
//! protocol dependencies. UI crates own presentation, helper binaries own the
//! protocol engines, and this crate owns the shared wire/model boundary.

mod helper_protocol;
mod model;
mod provider;
mod secret;

pub use helper_protocol::{
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState,
    RemoteDesktopMouseButton, RemoteDesktopMouseButtonState, RemoteDesktopWheelDelta,
};
pub use model::{
    RemoteDesktopConnectionProfile, RemoteDesktopEndpoint, RemoteDesktopFrame,
    RemoteDesktopFrameFormat, RemoteDesktopProtocol, RemoteDesktopSessionId,
    RemoteDesktopSessionStatus, RemoteDesktopSize,
};
pub use provider::{
    RemoteDesktopProviderCapabilities, RemoteDesktopProviderEntry, RemoteDesktopProviderError,
    RemoteDesktopProviderManifest, RemoteDesktopProviderRegistry, RemoteDesktopProviderUi,
};
pub use secret::RemoteDesktopSecret;

