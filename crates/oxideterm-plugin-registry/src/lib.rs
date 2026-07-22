// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native plugin registry crate.
//!
//! This crate owns native plugin discovery, package installation, contribution
//! indexing, config persistence, and validation. The GPUI app consumes this
//! crate as a host-facing boundary instead of carrying registry internals.

use std::{
    collections::HashMap,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde_json::Value;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

pub use oxideterm_plugin_manifest::{
    NativePluginActivityBarItemContribution, NativePluginActivityBarItemDef,
    NativePluginAiToolContribution, NativePluginAiToolDef, NativePluginApiCommandContribution,
    NativePluginConfigEntry, NativePluginConnectionHookContribution, NativePluginContributes,
    NativePluginDeclarativeUiColumn, NativePluginDeclarativeUiControl,
    NativePluginDeclarativeUiSchema, NativePluginDeclarativeUiSection, NativePluginDiagnostic,
    NativePluginGlobalConfig, NativePluginHostMonitorContribution, NativePluginHostMonitorDef,
    NativePluginHostMonitorOutputDef, NativePluginHostMonitorOutputFormat, NativePluginInfo,
    NativePluginInstalledInfo, NativePluginManifest, NativePluginPermissions,
    NativePluginProcessActivationPlan, NativePluginRegistryEntry, NativePluginRegistryIndex,
    NativePluginRuntime, NativePluginRuntimeActivityBarItemContribution,
    NativePluginRuntimeCommandContribution, NativePluginRuntimeContextMenuContribution,
    NativePluginRuntimeContextMenuItem, NativePluginRuntimeEventSubscriptionContribution,
    NativePluginRuntimeKeybindingContribution, NativePluginRuntimeKind, NativePluginRuntimePlan,
    NativePluginRuntimeSidebarPanelContribution, NativePluginRuntimeStatusItemContribution,
    NativePluginRuntimeTabViewContribution, NativePluginRuntimeTerminalHookContribution,
    NativePluginSettingContribution, NativePluginSettingDef, NativePluginSettingOption,
    NativePluginShortcutContribution, NativePluginShortcutDef, NativePluginSidebarContribution,
    NativePluginSidebarDef, NativePluginState, NativePluginTabContribution, NativePluginTabDef,
    NativePluginTerminalHooksDef, NativePluginTransportContribution, NativePluginUrlInstallResult,
    NativePluginWasmActivationPlan,
};
use oxideterm_plugin_protocol::{
    PluginOutboundMessage, PluginRegistration, PluginRegistrationKind, PluginRuntimeLogLevel,
};

mod constants;
mod contributions;
mod discovery;
mod install;
mod paths;
mod permissions;
mod registry;
mod state_semantics;
mod validation;

#[cfg(test)]
mod tests;

pub use constants::{
    NATIVE_PLUGIN_AI_MESSAGE_EVENT, NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT,
    NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT, NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT,
    NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT, NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT,
    NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT, NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT,
    NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT, NATIVE_PLUGIN_LIFECYCLE_CONNECT_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_DISCONNECT_EVENT, NATIVE_PLUGIN_LIFECYCLE_LINK_DOWN_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_RECONNECT_EVENT, NATIVE_PLUGIN_PROFILER_METRICS_EVENT,
    NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT, NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
    NATIVE_PLUGIN_SETTING_CHANGED_EVENT, NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT,
    NATIVE_PLUGIN_TRANSFER_ERROR_EVENT, NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
    NATIVE_PLUGIN_UI_EVENT, NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT,
};
pub use contributions::{
    NativePluginContributionStore, is_native_plugin_ai_tool_name, native_plugin_ai_tool_name,
};
pub use discovery::{load_native_plugin_config, save_native_plugin_config};
pub use paths::{native_plugin_config_path, native_plugins_dir};
pub use permissions::{
    NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY, native_plugin_capabilities_fingerprint,
    native_plugin_capability_approval_matches, native_plugin_requested_capabilities,
    native_plugin_requires_permission_review, normalize_native_plugin_capabilities,
};
pub use registry::NativePluginRegistry;
pub use state_semantics::{
    PLUGIN_ID_CONFLICT_ERROR_PREFIX, native_plugin_conflict_id, native_plugin_error_has_code,
    native_plugin_state_is_active_like, native_plugin_state_is_error_like,
};
pub use validation::{
    native_plugin_custom_event_key, native_plugin_declarative_control_is_actionable,
    native_plugin_runtime_subscription_event, native_plugin_state_for,
    native_plugin_state_for_manifest, native_runtime_kind_label, native_runtime_plan_for_manifest,
    validate_native_plugin_id, validate_plugin_relative_path,
};

// Internal modules intentionally share helper functions through the crate root;
// that keeps the split mechanical while the public API remains explicit above.
pub(crate) use constants::*;
pub(crate) use discovery::*;
pub(crate) use install::*;
pub(crate) use paths::*;
pub(crate) use validation::*;
