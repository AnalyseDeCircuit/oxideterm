// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use oxideterm_plugin_manifest::NativePluginManifest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    event::PluginEvent,
    permissions::PluginPermissionSet,
    sensitive::{PluginHostCallSensitivity, zeroize_json_value},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginActivateRequest {
    pub request_id: String,
    pub manifest: NativePluginManifest,
    pub permissions: PluginPermissionSet,
    pub timeout_ms: u64,
}

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRequest {
    pub request_id: String,
    pub kind: PluginRequestKind,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(PartialEq, Serialize, Deserialize)]
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

impl PluginRequest {
    pub fn zeroize_sensitive_host_call_args(&mut self) {
        if let PluginRequestKind::CallHostApi {
            namespace,
            method,
            args,
        } = &mut self.kind
            && PluginHostCallSensitivity::classify(namespace, method).is_sensitive()
        {
            zeroize_json_value(args);
        }
    }
}

impl fmt::Debug for PluginRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginRequest")
            .field("request_id", &self.request_id)
            .field("kind", &self.kind)
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}

impl fmt::Debug for PluginRequestKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Activate {
                manifest,
                permissions,
            } => formatter
                .debug_struct("Activate")
                .field("manifest", manifest)
                .field("permissions", permissions)
                .finish(),
            Self::Deactivate => formatter.write_str("Deactivate"),
            Self::CallHostApi {
                namespace,
                method,
                args,
            } => {
                let mut debug = formatter.debug_struct("CallHostApi");
                debug.field("namespace", namespace).field("method", method);
                if PluginHostCallSensitivity::classify(namespace, method).is_sensitive() {
                    debug.field("args", &"<redacted>");
                } else {
                    debug.field("args", args);
                }
                debug.finish()
            }
            Self::DispatchCommand { command, args } => formatter
                .debug_struct("DispatchCommand")
                .field("command", command)
                .field("args", args)
                .finish(),
            Self::SendEvent { event } => formatter
                .debug_struct("SendEvent")
                .field("event", event)
                .finish(),
            Self::CancelRequest { request_id } => formatter
                .debug_struct("CancelRequest")
                .field("request_id", request_id)
                .finish(),
            Self::Health => formatter.write_str("Health"),
            Self::Kill => formatter.write_str("Kill"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_host_call_request_debug_redacts_arguments() {
        let request = PluginRequest {
            request_id: "sync-1".to_string(),
            kind: PluginRequestKind::CallHostApi {
                namespace: "sync".to_string(),
                method: "exportOxide".to_string(),
                args: serde_json::json!({ "password": "sensitive-value" }),
            },
            timeout_ms: Some(1_000),
        };

        let rendered = format!("{request:?}");

        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("sensitive-value"));
    }

    #[test]
    fn sensitive_host_call_request_can_clear_owned_arguments() {
        let mut request = PluginRequest {
            request_id: "sync-2".to_string(),
            kind: PluginRequestKind::CallHostApi {
                namespace: "sync".to_string(),
                method: "previewImport".to_string(),
                args: serde_json::json!({ "password": "sensitive-value" }),
            },
            timeout_ms: Some(1_000),
        };

        request.zeroize_sensitive_host_call_args();

        let PluginRequestKind::CallHostApi { args, .. } = request.kind else {
            panic!("request kind should remain a host call");
        };
        assert!(args.is_null());
    }
}
