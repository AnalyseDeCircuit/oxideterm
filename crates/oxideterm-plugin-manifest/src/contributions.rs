// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde_json::Value;

use crate::manifest::{
    NativePluginAiToolDef, NativePluginDeclarativeUiSchema, NativePluginHostMonitorDef,
    NativePluginSettingDef, NativePluginShortcutDef, NativePluginSidebarDef, NativePluginTabDef,
};

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginTabContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginTabDef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginSidebarContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginSidebarDef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginSettingContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginSettingDef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginAiToolContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginAiToolDef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginShortcutContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginShortcutDef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginTransportContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub transport: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginConnectionHookContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub hook: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginApiCommandContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginHostMonitorContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub definition: NativePluginHostMonitorDef,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeCommandContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub command: String,
    pub label: String,
    pub icon: Option<String>,
    pub shortcut: Option<String>,
    pub section: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeKeybindingContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub keybinding: String,
    pub normalized_keybinding: String,
    pub command: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeTerminalHookContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeContextMenuContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub target: String,
    pub items: Vec<NativePluginRuntimeContextMenuItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeContextMenuItem {
    pub label: String,
    pub icon: Option<String>,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeStatusItemContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub text: String,
    pub icon: Option<String>,
    pub tooltip: Option<String>,
    pub alignment: String,
    pub priority: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeTabViewContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub tab_id: String,
    pub title: String,
    pub icon: String,
    pub schema: NativePluginDeclarativeUiSchema,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeSidebarPanelContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub panel_id: String,
    pub title: String,
    pub icon: String,
    pub position: String,
    pub schema: NativePluginDeclarativeUiSchema,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NativePluginRuntimeEventSubscriptionContribution {
    pub plugin_id: String,
    pub plugin_name: String,
    pub registration_id: String,
    pub event: String,
    pub filter: Option<Value>,
}
