// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::PluginError, event::PluginEvent};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum PluginOutboundMessage {
    RegisterContribution {
        registration: PluginRegistration,
    },
    DisposeContribution {
        registration_id: String,
    },
    Log {
        level: PluginRuntimeLogLevel,
        message: String,
    },
    ReportProgress {
        registration_id: String,
        value: Value,
    },
    RuntimeReady,
    RuntimeError {
        error: PluginError,
    },
    EmitEvent {
        event: PluginEvent,
    },
    CallHostApi {
        request_id: String,
        namespace: String,
        method: String,
        args: Value,
    },
}

impl fmt::Debug for PluginOutboundMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegisterContribution { registration } => formatter
                .debug_struct("RegisterContribution")
                .field("registration", registration)
                .finish(),
            Self::DisposeContribution { registration_id } => formatter
                .debug_struct("DisposeContribution")
                .field("registration_id", registration_id)
                .finish(),
            Self::Log { level, message } => formatter
                .debug_struct("Log")
                .field("level", level)
                .field("message", message)
                .finish(),
            Self::ReportProgress {
                registration_id,
                value,
            } => formatter
                .debug_struct("ReportProgress")
                .field("registration_id", registration_id)
                .field("value", value)
                .finish(),
            Self::RuntimeReady => formatter.write_str("RuntimeReady"),
            Self::RuntimeError { error } => formatter
                .debug_struct("RuntimeError")
                .field("error", error)
                .finish(),
            Self::EmitEvent { event } => formatter
                .debug_struct("EmitEvent")
                .field("event", event)
                .finish(),
            Self::CallHostApi {
                request_id,
                namespace,
                method,
                args,
            } => {
                let mut debug = formatter.debug_struct("CallHostApi");
                debug
                    .field("request_id", request_id)
                    .field("namespace", namespace)
                    .field("method", method);
                if namespace == "secrets" {
                    debug.field("args", &"<redacted>");
                } else {
                    debug.field("args", args);
                }
                debug.finish()
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRegistration {
    pub registration_id: String,
    pub plugin_id: String,
    pub kind: PluginRegistrationKind,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginRegistrationKind {
    Command,
    Keybinding,
    ContextMenu,
    StatusBar,
    Tab,
    SidebarPanel,
    ActivityBarItem,
    TerminalInputInterceptor,
    TerminalOutputProcessor,
    TerminalShortcut,
    EventSubscription,
    Progress,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginRuntimeLogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_bar_registration_kind_uses_stable_wire_name() {
        // The wire value is consumed by process and WASM plugins, so it must
        // remain kebab-cased independently of Rust enum naming.
        assert_eq!(
            serde_json::to_value(PluginRegistrationKind::ActivityBarItem).unwrap(),
            serde_json::json!("activity-bar-item")
        );
    }

    #[test]
    fn secret_host_call_message_debug_redacts_arguments() {
        let message = PluginOutboundMessage::CallHostApi {
            request_id: "secret-1".to_string(),
            namespace: "secrets".to_string(),
            method: "set".to_string(),
            args: serde_json::json!({ "key": "token", "value": "sensitive-value" }),
        };

        let rendered = format!("{message:?}");

        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("sensitive-value"));
    }
}
