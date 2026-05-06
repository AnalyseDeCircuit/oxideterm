// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! SSH port forwarding runtime for native OxideTerm.
//!
//! The shape mirrors the Tauri runtime: a registry owns per-session managers,
//! managers own active and stopped rules, and the concrete forward runners keep
//! SSH bridge state out of GPUI views.

mod bridge;
mod dynamic;
mod error;
mod local;
mod manager;
mod model;
mod registry;
mod remote;

pub use bridge::{
    ActiveConnectionCounter, BridgeStatsRecorder, DEFAULT_FORWARD_IDLE_TIMEOUT,
    FORWARD_BRIDGE_CHANNEL_CAPACITY, FORWARD_BRIDGE_READ_BUFFER_SIZE,
};
pub use error::ForwardingError;
pub use manager::ForwardingManager;
pub use model::{ForwardRule, ForwardStats, ForwardStatus, ForwardType, ForwardUpdate};
pub use registry::ForwardingRegistry;
