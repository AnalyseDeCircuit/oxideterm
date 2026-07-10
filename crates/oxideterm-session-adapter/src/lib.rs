// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Runtime adapters for saved connection records.
//!
//! This crate owns non-UI conversion from persisted connection/settings state
//! into SSH runtime configuration. GPUI views keep form state and rendering,
//! while this boundary hydrates secrets only when a runtime session needs them.

mod auth;
mod proxy;
mod runtime_settings;
mod ssh;

pub use auth::{auth_method_from_saved_auth, managed_key_resolver_from_store};
pub use proxy::upstream_proxy_config_from_saved_policy;
pub use runtime_settings::{
    reconnect_max_attempts_from_settings, reconnect_timing_from_settings,
    sftp_runtime_settings_from_settings, terminal_encoding_from_settings,
};
pub use ssh::{
    proxy_chain_config_from_saved_connection, ssh_config_for_saved_connection_hop,
    ssh_config_from_saved_connection,
};

#[cfg(test)]
mod tests;
