// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Protocol-neutral remote desktop domain primitives.
//!
//! This crate deliberately avoids GPUI, SSH handles, and concrete RDP/VNC
//! protocol dependencies. UI crates own presentation, helper binaries own the
//! protocol engines, and this crate owns the shared wire/model boundary.

mod codec;
mod fake;
mod helper_protocol;
mod model;
mod provider;
mod secret;

pub use codec::{
    RemoteDesktopJsonLineError, decode_event_line, decode_request_line, encode_event_line,
    encode_request_line, read_event_line, read_request_line, write_event_line, write_request_line,
};
pub use fake::{RemoteDesktopFakeBackend, run_fake_backend_stdio};
pub use helper_protocol::{
    RemoteDesktopClipboardData, RemoteDesktopClipboardFormat, RemoteDesktopErrorCategory,
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState,
    RemoteDesktopLockKeys, RemoteDesktopMouseButton, RemoteDesktopMouseButtonState,
    RemoteDesktopWheelDelta,
};
pub use model::{
    RemoteDesktopConnectionProfile, RemoteDesktopCursorShape, RemoteDesktopEndpoint,
    RemoteDesktopFrame, RemoteDesktopFrameCompression, RemoteDesktopFrameFormat,
    RemoteDesktopFrameUpdate, RemoteDesktopProtocol, RemoteDesktopRect, RemoteDesktopSessionId,
    RemoteDesktopSessionStatus, RemoteDesktopSize,
};
pub use provider::{
    RemoteDesktopProviderCapabilities, RemoteDesktopProviderEntry, RemoteDesktopProviderError,
    RemoteDesktopProviderManifest, RemoteDesktopProviderRegistry, RemoteDesktopProviderUi,
    builtin_preview_provider_manifest, builtin_preview_provider_registry,
    builtin_provider_manifest, builtin_provider_registry,
};
pub use secret::RemoteDesktopSecret;
