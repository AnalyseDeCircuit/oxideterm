// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OxideTerm-owned Mosh bootstrap and UDP session lifecycle.
//!
//! Mosh is intentionally a separate connection type. SSH is used only as an
//! authenticated, dedicated bootstrap transport and is released before this
//! crate starts driving the long-lived UDP session.

mod bootstrap;
mod session;

pub use bootstrap::{
    DEFAULT_MOSH_SERVER_EXECUTABLE, MoshBootstrapConfig, MoshBootstrapContext, MoshBootstrapError,
    MoshBootstrapResult, MoshIpFamily, MoshUdpPortSelection, bootstrap_mosh,
};
pub use session::{
    MoshSessionClient, MoshSessionCommandError, MoshSessionConfig, MoshSessionEvent,
    MoshSessionOwner, MoshSessionStartError, start_mosh_session,
};

pub use fernomade_runtime::{ConnectionState as MoshConnectionState, ShutdownOutcome};
