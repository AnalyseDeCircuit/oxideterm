// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_plugin_manifest::NativePluginManifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{event::PluginEvent, permissions::PluginPermissionSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActivateRequest {
    pub request_id: String,
    pub manifest: NativePluginManifest,
    pub permissions: PluginPermissionSet,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRequest {
    pub request_id: String,
    pub kind: PluginRequestKind,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PluginRequestKind {
    Activate {
        manifest: NativePluginManifest,
        permissions: PluginPermissionSet,
    },
    Deactivate,
    CallHostApi {
        namespace: String,
        method: String,
        args: Value,
    },
    DispatchCommand {
        command: String,
        args: Value,
    },
    SendEvent {
        event: PluginEvent,
    },
    CancelRequest {
        request_id: String,
    },
    Health,
    Kill,
}
