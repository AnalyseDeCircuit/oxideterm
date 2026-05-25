// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::PluginError, event::PluginEvent};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
