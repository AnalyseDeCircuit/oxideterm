// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Workspace shim for SFTP host APIs owned by oxideterm-plugin-host-api.

pub(super) use oxideterm_plugin_host_api::sftp::native_plugin_sftp_response;

#[cfg(test)]
pub(super) use oxideterm_plugin_host_api::sftp::{
    native_plugin_sftp_check_capability, native_plugin_sftp_node_id_arg,
    native_plugin_sftp_path_arg,
};
