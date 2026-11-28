// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native plugin manifest and registry data model.

mod config;
mod connection;
mod contributions;
pub mod discovery;
mod language;
mod manifest;
pub mod catalog;
mod registry;
mod runtime;

pub use config::{NativePluginConfigEntry, NativePluginGlobalConfig};
pub use connection::NativePluginConnectionDef;
pub use contributions::*;
pub use language::NativePluginLanguageDefinition;
pub use manifest::*;
pub use registry::*;
pub use runtime::{NativePluginRuntimePlan, NativePluginState};
