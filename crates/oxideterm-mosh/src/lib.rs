// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm-owned Mosh bootstrap and plugin process lifecycle.
//!
//! Mosh is intentionally a separate connection type. SSH is used only as an
//! authenticated, dedicated bootstrap transport and is released before this
//! crate launches the plugin's long-lived UDP session through its private pipe.

mod bootstrap;
mod session;
pub mod wire;

pub use bootstrap::{
    DEFAULT_MOSH_SERVER_EXECUTABLE, MoshBootstrapConfig, MoshBootstrapContext, MoshBootstrapError,
    MoshBootstrapResult, MoshIpFamily, MoshSessionKey, MoshUdpPortSelection, bootstrap_mosh,
};
pub use session::{
    MoshPluginSessions, MoshSessionCancellation, MoshSessionClient, MoshSessionCommandError,
    MoshSessionConfig, MoshSessionEvent, MoshSessionLease, MoshSessionOwner, MoshSessionStartError,
    start_mosh_session,
};

pub use wire::{ConnectionState as MoshConnectionState, ShutdownOutcome};
