// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Workspace shim for forwarding host APIs owned by oxideterm-plugin-host-api.

pub(super) use oxideterm_plugin_host_api::forwarding::{
    native_plugin_forward_response, native_plugin_forward_saved_forwards,
};

#[cfg(test)]
pub(super) use oxideterm_plugin_host_api::forwarding::{
    native_plugin_forward_check_capability, native_plugin_forward_create_request,
    native_plugin_forward_rule_snapshot,
};
