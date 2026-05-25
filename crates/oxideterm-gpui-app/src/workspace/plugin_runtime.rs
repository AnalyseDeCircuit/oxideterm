// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! GPUI workspace compatibility shim for the native plugin runtime host.
//!
//! The runtime host owns process/WASM execution, protocol dispatch, and
//! permission validation in `oxideterm-plugin-host-api`. Workspace code imports
//! this module while the remaining lifecycle UI bridge is being thinned.

pub(super) use oxideterm_plugin_host_api::runtime::*;
