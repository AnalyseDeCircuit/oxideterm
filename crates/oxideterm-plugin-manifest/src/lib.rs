// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native plugin manifest and registry data model.

mod config;
mod contributions;
mod manifest;
mod registry;
mod runtime;

pub use config::{NativePluginConfigEntry, NativePluginGlobalConfig};
pub use contributions::*;
pub use manifest::*;
pub use registry::*;
pub use runtime::{NativePluginRuntimePlan, NativePluginState};
