// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH port forwarding runtime for native OxideTerm.
//!
//! The shape mirrors the Tauri runtime: a registry owns per-session managers,
//! managers own active and stopped rules, and the concrete forward runners keep
//! SSH bridge state out of GPUI views.

mod bridge;
mod detection;
mod dynamic;
mod error;
mod events;
mod local;
mod manager;
mod model;
mod profiler;
mod registry;
mod remote;
mod saved;

pub use bridge::{
    ActiveConnectionCounter, BridgeStatsRecorder, DEFAULT_FORWARD_IDLE_TIMEOUT,
    FORWARD_BRIDGE_CHANNEL_CAPACITY, FORWARD_BRIDGE_READ_BUFFER_SIZE,
};
pub use detection::{DetectedPort, PortDetectionSnapshot, PortDetectionTracker};
pub use error::ForwardingError;
pub(crate) use error::{tauri_dynamic_bind_error, tauri_local_bind_error};
pub use events::ForwardEvent;
pub use manager::ForwardingManager;
pub use model::{ForwardRule, ForwardStats, ForwardStatus, ForwardType, ForwardUpdate};
pub use profiler::PortDetectionProfiler;
pub use registry::ForwardingRegistry;
pub use saved::{
    ApplySavedForwardsSyncSnapshotResult, DeletedPersistedForwardTombstone,
    FORWARD_TOMBSTONE_RETENTION_DAYS, PersistedForward, PersistedForwardDto, SavedForwardError,
    SavedForwardStore, SavedForwardSyncRecord, SavedForwardsSyncSnapshot,
};
