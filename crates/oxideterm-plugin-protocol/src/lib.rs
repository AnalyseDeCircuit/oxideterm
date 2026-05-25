// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared native plugin protocol types used by process and WASM runtimes.

mod envelope;
mod error;
mod event;
mod host_call;
mod message;
mod permissions;
mod request;
mod response;
mod runtime_state;
mod supervisor;

pub use envelope::{
    NATIVE_PLUGIN_PROTOCOL_VERSION, PluginProtocolEnvelope, validate_protocol_version,
};
pub use error::PluginError;
pub use event::PluginEvent;
pub use host_call::PluginHostCall;
pub use message::{
    PluginOutboundMessage, PluginRegistration, PluginRegistrationKind, PluginRuntimeLogLevel,
};
pub use permissions::PluginPermissionSet;
pub use request::{PluginActivateRequest, PluginRequest, PluginRequestKind};
pub use response::{PluginResponse, PluginResponseResult};
pub use runtime_state::{PluginRuntimeHealth, PluginRuntimeLifecycleState, PluginRuntimeLogEntry};
pub use supervisor::{PluginOutboundEffect, PluginRuntimeSupervisorState};
