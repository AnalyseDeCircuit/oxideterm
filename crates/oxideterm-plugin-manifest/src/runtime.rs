// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::NativePluginHelperDef;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativePluginRuntimePlan {
    ManifestOnly,
    Language {
        entry: String,
    },
    Wasm {
        entry: String,
    },
    Process {
        entry: String,
    },
    Helper {
        entry: String,
        definition: NativePluginHelperDef,
    },
    UnsupportedLegacyJs {
        entry: String,
    },
}

impl NativePluginRuntimePlan {
    /// The feature owner selects its own supported pipe rather than plugin messages.
    pub fn helper_entry(&self, feature: &str, protocol: &str, version: u32) -> Option<&str> {
        match self {
            Self::Helper { entry, definition }
                if definition.feature == feature
                    && definition.protocol == protocol
                    && definition.protocol_version == version =>
            {
                Some(entry)
            }
            _ => None,
        }
    }
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
