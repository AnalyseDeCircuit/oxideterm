// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Protocol-neutral remote desktop domain primitives.
//!
//! This crate deliberately avoids GPUI, SSH handles, and concrete RDP/VNC
//! protocol dependencies. UI crates own presentation, helper binaries own the
//! protocol engines, and this crate owns the shared wire/model boundary.

mod certificate_store;
mod codec;
mod fake;
mod frame_queue;
mod helper_process;
mod helper_protocol;
mod model;
mod provider;
mod request_writer;
mod secret;
mod worker;

pub use certificate_store::{
    REMOTE_DESKTOP_CERTIFICATE_STORE_FILE, RemoteDesktopCertificateStore, certificate_endpoint_key,
};
pub use codec::{
    RemoteDesktopJsonLineError, decode_event_line, decode_request_line, encode_event_line,
    encode_request_line, read_event_line, read_request_line, write_event_line, write_request_line,
};
pub use fake::{RemoteDesktopFakeBackend, run_fake_backend_stdio};
pub use frame_queue::{
    RemoteDesktopFrameDeliveryDecision, RemoteDesktopFrameDeliverySlot, RemoteDesktopFrameQueue,
    RemoteDesktopFrameQueuePush, is_remote_desktop_frame_event,
};
pub use helper_process::{ResolvedRemoteDesktopHelper, resolve_remote_desktop_helper_command};
pub use helper_protocol::{
    RemoteDesktopClipboardData, RemoteDesktopClipboardFormat, RemoteDesktopErrorCategory,
    RemoteDesktopHelperEvent, RemoteDesktopHelperRequest, RemoteDesktopKey, RemoteDesktopKeyState,
    RemoteDesktopLockKeys, RemoteDesktopMouseButton, RemoteDesktopMouseButtonState,
    RemoteDesktopServerCertificate, RemoteDesktopServerIdentityKind, RemoteDesktopWheelDelta,
};
pub use model::{
    NegotiatedCapabilities, NegotiatedCapabilityStatus, RemoteDesktopAudioOptions,
    RemoteDesktopClipboardOptions, RemoteDesktopConnectionProfile, RemoteDesktopCursorShape,
    RemoteDesktopDisplayOptions, RemoteDesktopEndpoint, RemoteDesktopFrame,
    RemoteDesktopFrameCompression, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopMonitor, RemoteDesktopMonitorLayout, RemoteDesktopMonitorOrientation,
    RemoteDesktopProtocol, RemoteDesktopRect, RemoteDesktopSessionId, RemoteDesktopSessionOptions,
    RemoteDesktopSessionStatus, RemoteDesktopSize, RemoteDesktopVncCompression,
    RemoteDesktopVncImageQuality, RemoteDesktopVncOptions, RemoteDesktopVncSecurityPolicy,
    RemoteDesktopVncSessionMode,
};
pub use provider::{
    RemoteDesktopProviderCapabilities, RemoteDesktopProviderEntry, RemoteDesktopProviderError,
    RemoteDesktopProviderManifest, RemoteDesktopProviderRegistry, RemoteDesktopProviderUi,
    builtin_preview_provider_manifest, builtin_preview_provider_registry,
    builtin_provider_manifest, builtin_provider_registry,
};
pub use secret::RemoteDesktopSecret;
pub use worker::{
    RemoteDesktopWorkerConfig, RemoteDesktopWorkerDelivery, RemoteDesktopWorkerId, connect_request,
    effective_session_options, initial_connect_request, remote_desktop_provider_uses_fake_backend,
    run_remote_desktop_worker,
};
