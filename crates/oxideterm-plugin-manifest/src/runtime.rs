// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativePluginRuntimePlan {
    ManifestOnly,
    Wasm { entry: String },
    Process { entry: String },
    UnsupportedLegacyJs { entry: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativePluginState {
    #[allow(dead_code)]
    Discovered,
    Disabled,
    UnsupportedLegacyJs,
    ReadyManifestOnly,
    ReadyWasm,
    ReadyProcess,
    #[allow(dead_code)]
    Loading,
    #[allow(dead_code)]
    Active,
    Error,
    AutoDisabled,
}
