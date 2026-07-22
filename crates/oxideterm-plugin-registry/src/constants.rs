// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared native plugin registry constants and event names.

pub(crate) const PLUGINS_DIR_NAME: &str = "plugins";
pub(crate) const PLUGIN_CONFIG_FILENAME: &str = "plugin-config.json";
pub(crate) const PLUGIN_CONFIG_CORRUPT_MARKER: &str = "corrupt";
pub(crate) const PLUGIN_MANIFEST_FILENAME: &str = "plugin.json";
#[cfg(test)]
pub(crate) const PLUGIN_CONFIG_SCHEMA_VERSION: u32 = 1;
pub(crate) const PLUGIN_STORAGE_MAX_KEY_BYTES: usize = 256;
pub(crate) const PLUGIN_STORAGE_MAX_PLUGIN_BYTES: usize = 256 * 1024;
#[allow(dead_code)]
pub(crate) const PLUGIN_PACKAGE_MAX_BYTES: u64 = 50 * 1024 * 1024;
#[allow(dead_code)]
pub(crate) const PLUGIN_PACKAGE_MAX_EXTRACTED_BYTES: u64 = 100 * 1024 * 1024;
#[allow(dead_code)]
pub(crate) const PLUGIN_PACKAGE_MAX_ENTRIES: usize = 2048;
pub const NATIVE_PLUGIN_UI_EVENT: &str = "ui.event";
pub(crate) const NATIVE_PLUGIN_DECLARATIVE_UI_FORM_KIND: &str = "form";
pub(crate) const NATIVE_PLUGIN_DECLARATIVE_UI_CONTROL_KINDS: &[&str] = &[
    "text",
    "password",
    "number",
    "checkbox",
    "select",
    "radioGroup",
    "radio-group",
    "slider",
    "segmentedControl",
    "segmented-control",
    "button",
    "iconButton",
    "icon-button",
    "stack",
    "row",
    "card",
    "toolbar",
    "alert",
    "markdown",
    "code",
    "codeBlock",
    "code-block",
    "statusBadge",
    "status-badge",
    "badge",
    "progress",
    "table",
    "list",
    "emptyState",
    "empty-state",
    "divider",
    "keyValue",
    "key-value",
    "keyValueRow",
    "key-value-row",
];
pub const NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT: &str = "app.themeChanged";
pub const NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT: &str = "app.settingsChanged";
pub const NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT: &str = "i18n.languageChanged";
pub const NATIVE_PLUGIN_SETTING_CHANGED_EVENT: &str = "settings.changed";
pub const NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT: &str = "ui.layoutChanged";
pub const NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT: &str = "sessions.treeChanged";
pub const NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT: &str = "sessions.nodeStateChanged";
pub const NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT: &str = "eventLog.entry";
pub const NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT: &str = "forward.savedForwardsChanged";
pub const NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT: &str = "transfers.progress";
pub const NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT: &str = "transfers.complete";
pub const NATIVE_PLUGIN_TRANSFER_ERROR_EVENT: &str = "transfers.error";
pub const NATIVE_PLUGIN_PROFILER_METRICS_EVENT: &str = "profiler.metrics";
pub const NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT: &str = "ide.fileOpen";
pub const NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT: &str = "ide.fileClose";
pub const NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT: &str = "ide.activeFileChanged";
pub const NATIVE_PLUGIN_AI_MESSAGE_EVENT: &str = "ai.message";
pub const NATIVE_PLUGIN_LIFECYCLE_CONNECT_EVENT: &str = "lifecycle.onConnect";
pub const NATIVE_PLUGIN_LIFECYCLE_DISCONNECT_EVENT: &str = "lifecycle.onDisconnect";
pub const NATIVE_PLUGIN_LIFECYCLE_LINK_DOWN_EVENT: &str = "lifecycle.onLinkDown";
pub const NATIVE_PLUGIN_LIFECYCLE_RECONNECT_EVENT: &str = "lifecycle.onReconnect";
pub(crate) const NATIVE_PLUGIN_PHASE4_SUBSCRIPTION_EVENTS: &[&str] = &[
    NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT,
    NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT,
    NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT,
    NATIVE_PLUGIN_SETTING_CHANGED_EVENT,
    NATIVE_PLUGIN_UI_LAYOUT_CHANGED_EVENT,
    NATIVE_PLUGIN_SESSION_TREE_CHANGED_EVENT,
    NATIVE_PLUGIN_SESSION_NODE_STATE_CHANGED_EVENT,
    NATIVE_PLUGIN_EVENT_LOG_ENTRY_EVENT,
    NATIVE_PLUGIN_FORWARD_SAVED_FORWARDS_CHANGED_EVENT,
    NATIVE_PLUGIN_TRANSFER_PROGRESS_EVENT,
    NATIVE_PLUGIN_TRANSFER_COMPLETE_EVENT,
    NATIVE_PLUGIN_TRANSFER_ERROR_EVENT,
    NATIVE_PLUGIN_PROFILER_METRICS_EVENT,
    NATIVE_PLUGIN_IDE_FILE_OPEN_EVENT,
    NATIVE_PLUGIN_IDE_FILE_CLOSE_EVENT,
    NATIVE_PLUGIN_IDE_ACTIVE_FILE_CHANGED_EVENT,
    NATIVE_PLUGIN_AI_MESSAGE_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_CONNECT_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_DISCONNECT_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_LINK_DOWN_EVENT,
    NATIVE_PLUGIN_LIFECYCLE_RECONNECT_EVENT,
];
