// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    error::PluginError,
    event::PluginEvent,
    sensitive::{PluginHostCallSensitivity, zeroize_json_value},
};

#[derive(PartialEq, Serialize, Deserialize)]
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

impl PluginOutboundMessage {
    pub fn host_call_sensitivity(&self) -> PluginHostCallSensitivity {
        match self {
            Self::CallHostApi {
                namespace, method, ..
            } => PluginHostCallSensitivity::classify(namespace, method),
            _ => PluginHostCallSensitivity::Public,
        }
    }

    pub fn zeroize_sensitive_host_call_args(&mut self) {
        if !self.host_call_sensitivity().is_sensitive() {
            return;
        }
        if let Self::CallHostApi { args, .. } = self {
            zeroize_json_value(args);
        }
    }

    pub fn clone_public(&self) -> Option<Self> {
        if self.host_call_sensitivity().is_sensitive() {
            return None;
        }
        Some(match self {
            Self::RegisterContribution { registration } => Self::RegisterContribution {
                registration: registration.clone(),
            },
            Self::DisposeContribution { registration_id } => Self::DisposeContribution {
                registration_id: registration_id.clone(),
            },
            Self::Log { level, message } => Self::Log {
                level: *level,
                message: message.clone(),
            },
            Self::ReportProgress {
                registration_id,
                value,
            } => Self::ReportProgress {
                registration_id: registration_id.clone(),
                value: value.clone(),
            },
            Self::RuntimeReady => Self::RuntimeReady,
            Self::RuntimeError { error } => Self::RuntimeError {
                error: error.clone(),
            },
            Self::EmitEvent { event } => Self::EmitEvent {
                event: event.clone(),
            },
            Self::CallHostApi {
                request_id,
                namespace,
                method,
                args,
            } => Self::CallHostApi {
                request_id: request_id.clone(),
                namespace: namespace.clone(),
                method: method.clone(),
                args: args.clone(),
            },
        })
    }
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
                if PluginHostCallSensitivity::classify(namespace, method).is_sensitive() {
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

    #[test]
    fn sync_password_host_call_cannot_use_public_clone_path() {
        let message = PluginOutboundMessage::CallHostApi {
            request_id: "sync-1".to_string(),
            namespace: "sync".to_string(),
            method: "exportOxide".to_string(),
            args: serde_json::json!({ "password": "sensitive-value" }),
        };

        assert!(message.clone_public().is_none());
        assert!(!format!("{message:?}").contains("sensitive-value"));
    }
}
