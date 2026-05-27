// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH port forwarding runtime for native OxideTerm.
//!
//! The shape mirrors the Tauri runtime: a registry owns per-session managers,
//! managers own active and stopped rules, and the concrete forward runners keep
//! SSH bridge state out of GPUI views.

#[cfg(feature = "runtime")]
mod bridge;
#[cfg(feature = "runtime")]
mod detection;
#[cfg(feature = "runtime")]
mod dynamic;
#[cfg(feature = "runtime")]
mod error;
#[cfg(feature = "runtime")]
mod events;
#[cfg(feature = "runtime")]
mod local;
#[cfg(feature = "runtime")]
mod manager;
mod model;
#[cfg(feature = "runtime")]
mod profiler;
#[cfg(feature = "runtime")]
mod registry;
#[cfg(not(feature = "runtime"))]
mod registry_saved;
#[cfg(feature = "runtime")]
mod remote;
mod saved;

#[cfg(feature = "runtime")]
pub use bridge::{
    ActiveConnectionCounter, BridgeStatsRecorder, DEFAULT_FORWARD_IDLE_TIMEOUT,
    FORWARD_BRIDGE_CHANNEL_CAPACITY, FORWARD_BRIDGE_READ_BUFFER_SIZE,
};
#[cfg(feature = "runtime")]
pub use detection::{DetectedPort, PortDetectionSnapshot, PortDetectionTracker};
#[cfg(feature = "runtime")]
pub use error::ForwardingError;
#[cfg(feature = "runtime")]
pub(crate) use error::{tauri_dynamic_bind_error, tauri_local_bind_error};
#[cfg(feature = "runtime")]
pub use events::ForwardEvent;
#[cfg(feature = "runtime")]
pub use manager::ForwardingManager;
pub use model::{ForwardRule, ForwardStats, ForwardStatus, ForwardType, ForwardUpdate};
#[cfg(feature = "runtime")]
pub use profiler::PortDetectionProfiler;
#[cfg(feature = "runtime")]
pub use registry::ForwardingRegistry;
#[cfg(not(feature = "runtime"))]
pub use registry_saved::ForwardingRegistry;
pub use saved::{
    ApplySavedForwardsSyncSnapshotResult, DeletedPersistedForwardTombstone,
    FORWARD_TOMBSTONE_RETENTION_DAYS, OwnedForwardImportRecord, PersistedForward,
    PersistedForwardDto, SavedForwardError, SavedForwardStore, SavedForwardSyncRecord,
    SavedForwardsSyncSnapshot,
};
