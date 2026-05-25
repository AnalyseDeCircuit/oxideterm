// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

pub(super) const NATIVE_PLUGIN_LIFECYCLE_TIMEOUT: Duration = Duration::from_secs(5);
pub(super) const NATIVE_PLUGIN_TERMINAL_HOOK_TIMEOUT: Duration = Duration::from_millis(5);
pub(super) const NATIVE_PLUGIN_DELIVERY_POLL_INTERVAL: Duration = Duration::from_millis(80);
pub(super) const NATIVE_PLUGIN_TRANSFER_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
pub(super) const NATIVE_PLUGIN_PROFILER_METRICS_INTERVAL: Duration = Duration::from_secs(1);
pub(super) const NATIVE_PLUGIN_TOAST_TTL: Duration = Duration::from_secs(4);

pub(super) use oxideterm_plugin_host_api::capabilities::{
    NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ, NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE,
    NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD,
};

pub(super) use oxideterm_plugin_host_api::backend::*;
