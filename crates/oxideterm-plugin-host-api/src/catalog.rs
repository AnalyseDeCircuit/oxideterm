// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Stable metadata for the direct plugin host API surface.

use std::collections::HashSet;

use serde::Serialize;
use serde_json::Value;

use crate::capabilities::*;

/// Security classification applied before a direct host API is made available.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AccessTier {
    BaselineRead,
    SensitiveRead,
    Mutating,
    Destructive,
    CredentialBroker,
}

/// Public metadata for one direct host call.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostApiDescriptor {
    pub namespace: &'static str,
    pub method: &'static str,
    pub access_tier: AccessTier,
    pub capability: Option<&'static str>,
    pub since: &'static str,
    pub summary: &'static str,
}

impl HostApiDescriptor {
    /// Returns the protocol name used by `PluginPermissionSet::allowed_host_apis`.
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", self.namespace, self.method)
    }
}

const CURRENT_API_VERSION: &str = env!("CARGO_PKG_VERSION");

macro_rules! api {
    ($namespace:literal, $method:literal, $tier:ident, $capability:expr, $summary:literal) => {
        HostApiDescriptor {
            namespace: $namespace,
            method: $method,
            access_tier: AccessTier::$tier,
            capability: $capability,
            since: CURRENT_API_VERSION,
            summary: $summary,
        }
    };
}

/// Complete catalog of direct host APIs implemented by the native plugin bridge.
pub static HOST_API_CATALOG: &[HostApiDescriptor] = &[
    api!(
        "app",
        "getTheme",
        BaselineRead,
        None,
        "Returns the active theme projection."
    ),
    api!(
        "theme",
        "getTokens",
        BaselineRead,
        None,
        "Returns the complete effective theme token set."
    ),
    api!(
        "theme",
        "getAvailable",
        BaselineRead,
        None,
        "Returns built-in and custom theme metadata."
    ),
    api!(
        "theme",
        "setActive",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_THEME_WRITE),
        "Changes the active application theme."
    ),
    api!(
        "app",
        "getSettings",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_APP_SETTINGS_READ),
        "Returns a host settings category."
    ),
    api!(
        "app",
        "getSettingsSummary",
        BaselineRead,
        None,
        "Returns an explicitly allowlisted host settings summary."
    ),
    api!(
        "app",
        "onThemeChange",
        BaselineRead,
        None,
        "Subscribes to theme metadata changes."
    ),
    api!(
        "app",
        "onSettingsChange",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_APP_SETTINGS_READ),
        "Subscribes to complete host settings snapshots."
    ),
    api!(
        "app",
        "getVersion",
        BaselineRead,
        None,
        "Returns the OxideTerm version."
    ),
    api!(
        "app",
        "getPlatform",
        BaselineRead,
        None,
        "Returns the current platform."
    ),
    api!(
        "app",
        "getLocale",
        BaselineRead,
        None,
        "Returns the current locale."
    ),
    api!(
        "app",
        "getApiCatalog",
        BaselineRead,
        None,
        "Returns the supported host API catalog and access tiers."
    ),
    api!(
        "app",
        "getPoolStats",
        BaselineRead,
        None,
        "Returns connection-pool statistics."
    ),
    api!(
        "app",
        "refreshAfterExternalSync",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_APP_SYNC_REFRESH),
        "Refreshes host state after an external sync."
    ),
    api!(
        "connections",
        "getAll",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_CONNECTIONS_READ),
        "Returns live connection projections."
    ),
    api!(
        "connections",
        "getSummaries",
        BaselineRead,
        None,
        "Returns redacted live-connection summaries."
    ),
    api!(
        "connections",
        "getSavedSummaries",
        BaselineRead,
        None,
        "Returns redacted saved-connection identities for discovery and connect workflows."
    ),
    api!(
        "connections",
        "getSaved",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_CONNECTIONS_READ),
        "Returns saved-connection projections without credential material or local key paths."
    ),
    api!(
        "connections",
        "get",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_CONNECTIONS_READ),
        "Returns one saved connection projection."
    ),
    api!(
        "connections",
        "getState",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_CONNECTIONS_READ),
        "Returns a connection state projection including failure detail."
    ),
    api!(
        "connections",
        "getByNode",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_CONNECTIONS_READ),
        "Returns the saved connection for a node."
    ),
    api!(
        "connections",
        "connect",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_CONNECTIONS_CONTROL),
        "Starts the existing saved-connection flow."
    ),
    api!(
        "connections",
        "reconnect",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_CONNECTIONS_CONTROL),
        "Schedules reconnect for an existing node."
    ),
    api!(
        "connections",
        "disconnect",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_CONNECTIONS_CONTROL),
        "Explicitly disconnects a node subtree through NodeRouter ownership."
    ),
    api!(
        "sessions",
        "getTree",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SESSIONS_READ),
        "Returns the session tree projection."
    ),
    api!(
        "sessions",
        "getSummary",
        BaselineRead,
        None,
        "Returns a redacted session summary."
    ),
    api!(
        "sessions",
        "getActiveNodes",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SESSIONS_READ),
        "Returns active node projections including endpoint metadata."
    ),
    api!(
        "sessions",
        "getNodeState",
        BaselineRead,
        None,
        "Returns a node state projection."
    ),
    api!(
        "sessions",
        "onTreeChange",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SESSIONS_READ),
        "Subscribes to complete session-tree projections."
    ),
    api!(
        "sessions",
        "onNodeStateChange",
        BaselineRead,
        None,
        "Subscribes to node identifier and state changes."
    ),
    api!(
        "eventLog",
        "getEntries",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SESSIONS_READ),
        "Returns filtered host event-log entries."
    ),
    api!(
        "eventLog",
        "getSummary",
        BaselineRead,
        None,
        "Returns event counts without event content or sources."
    ),
    api!(
        "eventLog",
        "onEntry",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SESSIONS_READ),
        "Subscribes to complete event-log entries."
    ),
    api!(
        "notifications",
        "getSummary",
        BaselineRead,
        None,
        "Returns notification counts without notification content."
    ),
    api!(
        "notifications",
        "getAll",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_NOTIFICATIONS_READ),
        "Returns notification content and scopes."
    ),
    api!(
        "notifications",
        "markRead",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_NOTIFICATIONS_MANAGE),
        "Marks one notification as read."
    ),
    api!(
        "notifications",
        "markAllRead",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_NOTIFICATIONS_MANAGE),
        "Marks all notifications as read."
    ),
    api!(
        "notifications",
        "setDnd",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_NOTIFICATIONS_MANAGE),
        "Changes notification do-not-disturb state."
    ),
    api!(
        "notifications",
        "remove",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_NOTIFICATIONS_MANAGE),
        "Removes one notification."
    ),
    api!(
        "notifications",
        "clear",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_NOTIFICATIONS_MANAGE),
        "Clears all notifications."
    ),
    api!(
        "cloudSync",
        "getSummary",
        BaselineRead,
        None,
        "Returns Cloud Sync status without destinations, credentials, errors, or content."
    ),
    api!(
        "cloudSync",
        "getHistory",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_CLOUD_SYNC_READ),
        "Returns sanitized Cloud Sync operation history."
    ),
    api!(
        "cloudSync",
        "check",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_CLOUD_SYNC_CONTROL),
        "Checks the configured Cloud Sync backend."
    ),
    api!(
        "cloudSync",
        "upload",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_CLOUD_SYNC_CONTROL),
        "Uploads the current synchronized product state."
    ),
    api!(
        "cloudSync",
        "pullPreview",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_CLOUD_SYNC_CONTROL),
        "Loads a remote-state preview without applying it."
    ),
    api!(
        "cloudSync",
        "applyPreview",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_CLOUD_SYNC_APPLY),
        "Applies the currently reviewed remote preview."
    ),
    api!(
        "cloudSync",
        "setAutoUpload",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_CLOUD_SYNC_CONTROL),
        "Changes automatic upload state."
    ),
    api!(
        "quickCommands",
        "getMetadata",
        BaselineRead,
        None,
        "Returns quick-command discovery metadata without executable content."
    ),
    api!(
        "quickCommands",
        "getAll",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_QUICK_COMMANDS_READ),
        "Returns complete quick-command definitions."
    ),
    api!(
        "quickCommands",
        "execute",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_QUICK_COMMANDS_EXECUTE),
        "Executes a saved quick command through the active terminal workflow."
    ),
    api!(
        "quickCommands",
        "upsert",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_QUICK_COMMANDS_MANAGE),
        "Creates or updates a quick command."
    ),
    api!(
        "quickCommands",
        "remove",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_QUICK_COMMANDS_MANAGE),
        "Removes a quick command."
    ),
    api!(
        "hostTools",
        "getSnapshot",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_READ),
        "Returns cached host metrics, processes, containers, and services for a node."
    ),
    api!(
        "hostTools",
        "getExtensions",
        BaselineRead,
        None,
        "Returns the requesting plugin's declared Host Tools monitor metadata without command text."
    ),
    api!(
        "hostTools",
        "capture",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_READ),
        "Captures one typed Host Tools dataset through the existing node connection."
    ),
    api!(
        "hostTools",
        "execute",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_WRITE),
        "Runs a validated non-destructive Host Tools action."
    ),
    api!(
        "hostTools",
        "terminate",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_DESTRUCTIVE),
        "Runs a validated destructive process or tmux action."
    ),
    api!(
        "hostTools",
        "runExtension",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_CUSTOM_EXECUTE),
        "Runs one static Host Tools monitor command declared by the requesting plugin."
    ),
    api!(
        "terminal",
        "getActiveTarget",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ),
        "Returns the active terminal target including its user-visible label."
    ),
    api!(
        "terminal",
        "getMetadata",
        BaselineRead,
        None,
        "Returns redacted terminal metadata."
    ),
    api!(
        "terminal",
        "getNodeBuffer",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ),
        "Returns terminal buffer content for a node."
    ),
    api!(
        "terminal",
        "getNodeSelection",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ),
        "Returns selected terminal content for a node."
    ),
    api!(
        "terminal",
        "search",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ),
        "Searches terminal buffer content."
    ),
    api!(
        "terminal",
        "getScrollBuffer",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ),
        "Returns a terminal scroll-buffer window."
    ),
    api!(
        "terminal",
        "getBufferSize",
        BaselineRead,
        None,
        "Returns terminal buffer dimensions."
    ),
    api!(
        "terminal",
        "writeToActive",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_WRITE),
        "Writes input to the active terminal."
    ),
    api!(
        "terminal",
        "writeToNode",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_WRITE),
        "Writes input to a node terminal."
    ),
    api!(
        "terminal",
        "clearBuffer",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_WRITE),
        "Clears a terminal buffer."
    ),
    api!(
        "terminal",
        "openTelnet",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_TERMINAL_WRITE),
        "Opens a declared Telnet transport."
    ),
    api!(
        "sftp",
        "init",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Initializes remote file access and returns its working directory."
    ),
    api!(
        "sftp",
        "listDir",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Lists a remote directory."
    ),
    api!(
        "sftp",
        "stat",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Returns remote file metadata."
    ),
    api!(
        "sftp",
        "readFile",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Reads remote file content."
    ),
    api!(
        "sftp",
        "preview",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Returns a preview of remote file content."
    ),
    api!(
        "sftp",
        "download",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Downloads a remote file to a local path."
    ),
    api!(
        "sftp",
        "downloadDir",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Downloads a remote directory to a local path."
    ),
    api!(
        "sftp",
        "tarProbe",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Checks remote tar support for directory transfers."
    ),
    api!(
        "sftp",
        "tarDownload",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
        "Downloads a remote directory through a tar stream."
    ),
    api!(
        "sftp",
        "writeFile",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
        "Writes remote file content."
    ),
    api!(
        "sftp",
        "write",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
        "Writes encoded content to a remote file."
    ),
    api!(
        "sftp",
        "upload",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
        "Uploads a local file to a remote path."
    ),
    api!(
        "sftp",
        "uploadDir",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
        "Uploads a local directory to a remote path."
    ),
    api!(
        "sftp",
        "tarUpload",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
        "Uploads a local directory through a tar stream."
    ),
    api!(
        "sftp",
        "mkdir",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
        "Creates a remote directory."
    ),
    api!(
        "sftp",
        "delete",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_DELETE),
        "Deletes a remote path."
    ),
    api!(
        "sftp",
        "deleteRecursive",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_DELETE),
        "Recursively deletes a remote path."
    ),
    api!(
        "sftp",
        "rename",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
        "Renames a remote path."
    ),
    api!(
        "forward",
        "list",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD_READ),
        "Lists active forwarding rules."
    ),
    api!(
        "forward",
        "getSummary",
        BaselineRead,
        None,
        "Returns forwarding rule counts without network endpoints."
    ),
    api!(
        "forward",
        "listSavedForwards",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD_READ),
        "Lists saved forwarding rules."
    ),
    api!(
        "forward",
        "onSavedForwardsChange",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD_READ),
        "Subscribes to saved-forward changes."
    ),
    api!(
        "forward",
        "exportSavedForwardsSnapshot",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD_READ),
        "Exports saved forwarding rules."
    ),
    api!(
        "forward",
        "applySavedForwardsSnapshot",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD),
        "Applies saved forwarding rules."
    ),
    api!(
        "forward",
        "create",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD),
        "Creates a forwarding rule."
    ),
    api!(
        "forward",
        "stop",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD),
        "Stops a forwarding rule."
    ),
    api!(
        "forward",
        "stopAll",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD),
        "Stops all forwarding rules."
    ),
    api!(
        "forward",
        "delete",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD),
        "Deletes a forwarding rule."
    ),
    api!(
        "forward",
        "restart",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD),
        "Restarts a forwarding rule."
    ),
    api!(
        "forward",
        "update",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD),
        "Updates a forwarding rule."
    ),
    api!(
        "forward",
        "getStats",
        BaselineRead,
        None,
        "Returns forwarding traffic statistics."
    ),
    api!(
        "secrets",
        "get",
        CredentialBroker,
        Some(NATIVE_PLUGIN_CAPABILITY_CREDENTIALS_RAW_READ),
        "Retrieves a plugin-scoped raw secret after explicit sensitive-content approval."
    ),
    api!(
        "secrets",
        "getMany",
        CredentialBroker,
        Some(NATIVE_PLUGIN_CAPABILITY_CREDENTIALS_RAW_READ),
        "Retrieves plugin-scoped raw secrets after explicit sensitive-content approval."
    ),
    api!(
        "secrets",
        "set",
        CredentialBroker,
        Some(NATIVE_PLUGIN_CAPABILITY_CREDENTIALS_MANAGE),
        "Stores a plugin-scoped secret through the host broker."
    ),
    api!(
        "secrets",
        "has",
        CredentialBroker,
        Some(NATIVE_PLUGIN_CAPABILITY_CREDENTIALS_MANAGE),
        "Checks for a plugin-scoped secret through the host broker."
    ),
    api!(
        "secrets",
        "delete",
        CredentialBroker,
        Some(NATIVE_PLUGIN_CAPABILITY_CREDENTIALS_MANAGE),
        "Deletes a plugin-scoped secret through the host broker."
    ),
    api!(
        "sync",
        "listSavedConnections",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_READ),
        "Lists saved connections for synchronization."
    ),
    api!(
        "sync",
        "refreshSavedConnections",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_WRITE),
        "Refreshes saved connections from storage."
    ),
    api!(
        "sync",
        "exportSavedConnectionsSnapshot",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_READ),
        "Exports a saved-connection snapshot."
    ),
    api!(
        "sync",
        "applySavedConnectionsSnapshot",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_WRITE),
        "Applies a saved-connection snapshot."
    ),
    api!(
        "sync",
        "getLocalSyncMetadata",
        BaselineRead,
        None,
        "Returns local synchronization metadata."
    ),
    api!(
        "sync",
        "preflightExport",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_READ),
        "Preflights an Oxide export."
    ),
    api!(
        "sync",
        "exportOxide",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_READ),
        "Exports an Oxide configuration package."
    ),
    api!(
        "sync",
        "validateOxide",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_READ),
        "Validates an Oxide configuration package."
    ),
    api!(
        "sync",
        "previewImport",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_READ),
        "Previews an Oxide configuration import."
    ),
    api!(
        "sync",
        "importOxide",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_WRITE),
        "Imports an Oxide configuration package."
    ),
    api!(
        "transfers",
        "getAll",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TRANSFERS_READ),
        "Returns all transfer projections."
    ),
    api!(
        "transfers",
        "getSummary",
        BaselineRead,
        None,
        "Returns transfer state counts without paths or errors."
    ),
    api!(
        "transfers",
        "getByNode",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TRANSFERS_READ),
        "Returns transfer projections for a node."
    ),
    api!(
        "transfers",
        "onProgress",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TRANSFERS_READ),
        "Subscribes to transfer progress."
    ),
    api!(
        "transfers",
        "onComplete",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TRANSFERS_READ),
        "Subscribes to transfer completion."
    ),
    api!(
        "transfers",
        "onError",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_TRANSFERS_READ),
        "Subscribes to transfer failures."
    ),
    api!(
        "profiler",
        "getMetrics",
        BaselineRead,
        None,
        "Returns current node metrics."
    ),
    api!(
        "profiler",
        "getHistory",
        BaselineRead,
        None,
        "Returns node metric history."
    ),
    api!(
        "profiler",
        "isRunning",
        BaselineRead,
        None,
        "Returns profiler running state."
    ),
    api!(
        "profiler",
        "onMetrics",
        BaselineRead,
        None,
        "Subscribes to node metrics."
    ),
    api!(
        "ide",
        "isOpen",
        BaselineRead,
        None,
        "Returns whether the IDE is open."
    ),
    api!(
        "ide",
        "getSummary",
        BaselineRead,
        None,
        "Returns a redacted IDE state summary."
    ),
    api!(
        "ide",
        "getProject",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_READ),
        "Returns the current IDE project projection."
    ),
    api!(
        "ide",
        "getOpenFiles",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_READ),
        "Returns open IDE file projections."
    ),
    api!(
        "ide",
        "getActiveFile",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_READ),
        "Returns the active IDE file projection."
    ),
    api!(
        "ide",
        "onFileOpen",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_READ),
        "Subscribes to IDE file-open events."
    ),
    api!(
        "ide",
        "onFileClose",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_READ),
        "Subscribes to IDE file-close events."
    ),
    api!(
        "ide",
        "onActiveFileChange",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_READ),
        "Subscribes to active IDE file changes."
    ),
    api!(
        "ide",
        "openFile",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_WRITE),
        "Opens a remote file in its existing IDE project."
    ),
    api!(
        "ide",
        "replaceActiveText",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_WRITE),
        "Replaces the active editor buffer without bypassing normal save semantics."
    ),
    api!(
        "ide",
        "insertActiveText",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_WRITE),
        "Inserts text at the active editor selection."
    ),
    api!(
        "ide",
        "saveActive",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_WRITE),
        "Saves the active IDE buffer through the existing remote file owner."
    ),
    api!(
        "ide",
        "closeFile",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_WRITE),
        "Closes an IDE file through the normal dirty-buffer confirmation flow."
    ),
    api!(
        "ide",
        "refreshProject",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_IDE_WRITE),
        "Refreshes the active IDE project tree."
    ),
    api!(
        "ai",
        "getConversations",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_AI_CONTENT_READ),
        "Returns AI conversation projections."
    ),
    api!(
        "ai",
        "getCatalog",
        BaselineRead,
        None,
        "Returns the redacted AI provider and model catalog."
    ),
    api!(
        "ai",
        "getMessages",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_AI_CONTENT_READ),
        "Returns AI message content."
    ),
    api!(
        "ai",
        "getActiveProvider",
        BaselineRead,
        None,
        "Returns the active AI provider projection."
    ),
    api!(
        "ai",
        "getAvailableModels",
        BaselineRead,
        None,
        "Returns available AI model projections."
    ),
    api!(
        "ai",
        "onMessage",
        BaselineRead,
        None,
        "Subscribes to AI message metadata without message content."
    ),
    api!(
        "ai",
        "createConversation",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_AI_WRITE),
        "Creates a conversation in the existing AI workspace."
    ),
    api!(
        "ai",
        "selectConversation",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_AI_WRITE),
        "Selects an existing AI conversation."
    ),
    api!(
        "ai",
        "sendMessage",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_AI_WRITE),
        "Sends plugin-provided text through the configured AI workflow."
    ),
    api!(
        "ai",
        "cancelGeneration",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_AI_WRITE),
        "Cancels the active AI generation."
    ),
    api!(
        "ai",
        "deleteConversation",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_AI_WRITE),
        "Deletes one AI conversation."
    ),
    api!(
        "ai",
        "clearConversations",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_AI_WRITE),
        "Clears all AI conversations."
    ),
    api!(
        "api",
        "invoke",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_LEGACY_INVOKE),
        "Invokes the legacy backend adapter."
    ),
    api!(
        "events",
        "emit",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_EVENTS_EMIT),
        "Emits a plugin-scoped custom event."
    ),
    api!(
        "events",
        "on",
        BaselineRead,
        None,
        "Subscribes to a plugin-scoped custom event."
    ),
    api!(
        "events",
        "onConnect",
        BaselineRead,
        None,
        "Subscribes to connection lifecycle notifications."
    ),
    api!(
        "events",
        "onDisconnect",
        BaselineRead,
        None,
        "Subscribes to disconnection lifecycle notifications."
    ),
    api!(
        "events",
        "onLinkDown",
        BaselineRead,
        None,
        "Subscribes to link-down lifecycle notifications."
    ),
    api!(
        "events",
        "onReconnect",
        BaselineRead,
        None,
        "Subscribes to reconnection lifecycle notifications."
    ),
    api!(
        "i18n",
        "t",
        BaselineRead,
        None,
        "Translates a plugin localization key."
    ),
    api!(
        "i18n",
        "getLanguage",
        BaselineRead,
        None,
        "Returns the active language."
    ),
    api!(
        "i18n",
        "onLanguageChange",
        BaselineRead,
        None,
        "Subscribes to active-language changes."
    ),
    api!(
        "settings",
        "get",
        BaselineRead,
        None,
        "Returns a plugin-scoped setting."
    ),
    api!(
        "settings",
        "set",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_PLUGIN_SETTINGS_WRITE),
        "Writes a plugin-scoped setting."
    ),
    api!(
        "settings",
        "onChange",
        BaselineRead,
        None,
        "Subscribes to this plugin's setting changes."
    ),
    api!(
        "settings",
        "exportSyncableSettings",
        SensitiveRead,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_READ),
        "Exports syncable plugin settings."
    ),
    api!(
        "settings",
        "applySyncableSettings",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_SYNC_WRITE),
        "Applies syncable plugin settings."
    ),
    api!(
        "ui",
        "getLayout",
        BaselineRead,
        None,
        "Returns the workspace layout projection."
    ),
    api!(
        "ui",
        "onLayoutChange",
        BaselineRead,
        None,
        "Subscribes to workspace layout projections."
    ),
    api!(
        "ui",
        "registerTabView",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_UI_WRITE),
        "Registers a declarative tab view."
    ),
    api!(
        "ui",
        "registerSidebarPanel",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_UI_WRITE),
        "Registers a declarative sidebar panel."
    ),
    api!(
        "ui",
        "registerActivityBarItem",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_UI_WRITE),
        "Registers a manifest-declared activity-bar command item."
    ),
    api!(
        "ui",
        "openTab",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_UI_WRITE),
        "Opens a declared plugin tab."
    ),
    api!(
        "ui",
        "showToast",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_UI_WRITE),
        "Shows a transient toast."
    ),
    api!(
        "ui",
        "showConfirm",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_UI_WRITE),
        "Shows a confirmation prompt."
    ),
    api!(
        "ui",
        "showProgress",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_UI_WRITE),
        "Shows or updates progress UI."
    ),
    api!(
        "ui",
        "showNotification",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_UI_WRITE),
        "Shows a host notification."
    ),
    api!(
        "storage",
        "set",
        Mutating,
        Some(NATIVE_PLUGIN_CAPABILITY_PLUGIN_SETTINGS_WRITE),
        "Writes plugin-scoped storage."
    ),
    api!(
        "storage",
        "remove",
        Destructive,
        Some(NATIVE_PLUGIN_CAPABILITY_PLUGIN_SETTINGS_WRITE),
        "Removes plugin-scoped storage."
    ),
    api!(
        "storage",
        "get",
        BaselineRead,
        None,
        "Returns plugin-scoped storage."
    ),
];

/// Returns the catalog in its plugin-facing JSON representation.
pub fn host_api_catalog_json() -> Value {
    serde_json::to_value(HOST_API_CATALOG).expect("static host API descriptors must serialize")
}

/// Returns whether a capability controls at least one supported host API.
pub fn is_supported_host_api_capability(capability: &str) -> bool {
    capability == NATIVE_PLUGIN_CAPABILITY_NETWORK_HTTP
        || HOST_API_CATALOG
            .iter()
            .any(|descriptor| descriptor.capability == Some(capability))
}

/// Builds the host API allowlist for a set of granted capabilities.
///
/// Baseline reads are always included so a plugin has a useful default data
/// plane. Every other entry requires its descriptor capability.
pub fn allowed_host_apis_for_capabilities<I, S>(capabilities: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut granted = capabilities
        .into_iter()
        .map(|capability| capability.as_ref().to_string())
        .collect::<HashSet<_>>();
    // Forward-management plugins necessarily inspect the rules they manage.
    if granted.contains(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD) {
        granted.insert(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD_READ.to_string());
    }
    HOST_API_CATALOG
        .iter()
        .filter(|descriptor| {
            descriptor.access_tier == AccessTier::BaselineRead
                || descriptor
                    .capability
                    .is_some_and(|capability| granted.contains(capability))
        })
        .map(HostApiDescriptor::qualified_name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_unique_complete_direct_api_names() {
        let names = HOST_API_CATALOG
            .iter()
            .map(HostApiDescriptor::qualified_name)
            .collect::<HashSet<_>>();

        assert_eq!(HOST_API_CATALOG.len(), 179);
        assert_eq!(names.len(), HOST_API_CATALOG.len());
        assert!(names.contains("api.invoke"));
        assert!(names.contains("connections.getSummaries"));
        assert!(names.contains("connections.getSavedSummaries"));
        assert!(names.contains("connections.getSaved"));
        assert!(names.contains("app.getApiCatalog"));
        assert!(names.contains("app.getSettingsSummary"));
        assert!(names.contains("eventLog.getSummary"));
        assert!(names.contains("notifications.getSummary"));
        assert!(names.contains("cloudSync.getSummary"));
        assert!(names.contains("quickCommands.getMetadata"));
        assert!(names.contains("theme.getTokens"));
        assert!(names.contains("theme.getAvailable"));
        assert!(names.contains("connections.disconnect"));
        assert!(names.contains("hostTools.capture"));
        assert!(names.contains("hostTools.getExtensions"));
        assert!(names.contains("hostTools.runExtension"));
        assert!(names.contains("quickCommands.execute"));
        assert!(names.contains("ide.replaceActiveText"));
        assert!(names.contains("ai.sendMessage"));
        assert!(names.contains("cloudSync.applyPreview"));
        assert!(names.contains("forward.getSummary"));
        assert!(names.contains("transfers.getSummary"));
        assert!(names.contains("sessions.getSummary"));
        assert!(names.contains("terminal.getMetadata"));
        assert!(names.contains("ai.getCatalog"));
        assert!(names.contains("ide.getSummary"));
        assert!(names.contains("terminal.getNodeBuffer"));
        assert!(names.contains("secrets.getMany"));
        assert!(names.contains("storage.get"));
        assert!(names.contains("ui.registerActivityBarItem"));
        for api in [
            "sftp.init",
            "sftp.preview",
            "sftp.download",
            "sftp.downloadDir",
            "sftp.tarProbe",
            "sftp.tarDownload",
            "sftp.write",
            "sftp.upload",
            "sftp.uploadDir",
            "sftp.tarUpload",
            "sftp.deleteRecursive",
            "forward.delete",
            "forward.restart",
            "forward.update",
        ] {
            assert!(names.contains(api));
        }
    }

    #[test]
    fn serialized_catalog_uses_stable_plugin_facing_fields() {
        let catalog = host_api_catalog_json();
        let entries = catalog.as_array().expect("catalog must be an array");
        let legacy = entries
            .iter()
            .find(|entry| entry["namespace"] == "api" && entry["method"] == "invoke")
            .expect("legacy API must be cataloged");

        assert_eq!(legacy["accessTier"], "mutating");
        assert_eq!(legacy["capability"], NATIVE_PLUGIN_CAPABILITY_LEGACY_INVOKE);
        assert_eq!(legacy["since"], CURRENT_API_VERSION);
        assert!(
            legacy["summary"]
                .as_str()
                .is_some_and(|value| !value.is_empty())
        );
    }

    #[test]
    fn capability_filter_keeps_baseline_data_and_gates_sensitive_calls() {
        let baseline = allowed_host_apis_for_capabilities(std::iter::empty::<&str>());
        assert!(baseline.contains(&"app.getVersion".to_string()));
        assert!(baseline.contains(&"terminal.getBufferSize".to_string()));
        for api in [
            "connections.getSummaries",
            "connections.getSavedSummaries",
            "sessions.getSummary",
            "terminal.getMetadata",
            "ai.getCatalog",
            "ide.getSummary",
            "app.getSettingsSummary",
            "eventLog.getSummary",
            "notifications.getSummary",
            "cloudSync.getSummary",
            "quickCommands.getMetadata",
            "theme.getTokens",
            "hostTools.getExtensions",
            "forward.getSummary",
            "transfers.getSummary",
        ] {
            assert!(baseline.contains(&api.to_string()));
        }
        assert!(!baseline.contains(&"terminal.getNodeBuffer".to_string()));
        assert!(!baseline.contains(&"terminal.writeToActive".to_string()));
        assert!(!baseline.contains(&"hostTools.capture".to_string()));
        assert!(!baseline.contains(&"hostTools.runExtension".to_string()));
        assert!(!baseline.contains(&"ide.replaceActiveText".to_string()));
        assert!(!baseline.contains(&"cloudSync.applyPreview".to_string()));
        assert!(!baseline.contains(&"ui.registerActivityBarItem".to_string()));

        let terminal = allowed_host_apis_for_capabilities([
            NATIVE_PLUGIN_CAPABILITY_TERMINAL_CONTENT_READ,
            NATIVE_PLUGIN_CAPABILITY_TERMINAL_WRITE,
        ]);
        assert!(terminal.contains(&"app.getVersion".to_string()));
        assert!(terminal.contains(&"terminal.getNodeBuffer".to_string()));
        assert!(terminal.contains(&"terminal.writeToActive".to_string()));
        assert!(!terminal.contains(&"secrets.get".to_string()));

        let product = allowed_host_apis_for_capabilities([
            NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_READ,
            NATIVE_PLUGIN_CAPABILITY_IDE_WRITE,
            NATIVE_PLUGIN_CAPABILITY_CLOUD_SYNC_APPLY,
            NATIVE_PLUGIN_CAPABILITY_HOST_TOOLS_CUSTOM_EXECUTE,
        ]);
        assert!(product.contains(&"hostTools.capture".to_string()));
        assert!(product.contains(&"hostTools.runExtension".to_string()));
        assert!(product.contains(&"ide.replaceActiveText".to_string()));
        assert!(product.contains(&"cloudSync.applyPreview".to_string()));
        assert!(!product.contains(&"hostTools.terminate".to_string()));

        let ui = allowed_host_apis_for_capabilities([NATIVE_PLUGIN_CAPABILITY_UI_WRITE]);
        assert!(ui.contains(&"ui.registerActivityBarItem".to_string()));

        let forward =
            allowed_host_apis_for_capabilities([NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD]);
        assert!(forward.contains(&"forward.create".to_string()));
        assert!(forward.contains(&"forward.listSavedForwards".to_string()));
    }

    #[test]
    fn every_non_baseline_api_has_an_explicit_capability() {
        assert!(HOST_API_CATALOG.iter().all(|descriptor| {
            descriptor.access_tier == AccessTier::BaselineRead || descriptor.capability.is_some()
        }));
    }

    #[test]
    fn direct_sftp_and_forward_apis_use_their_specific_capabilities() {
        for (method, capability) in [
            ("init", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
            ("preview", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
            ("download", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
            ("downloadDir", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
            ("tarProbe", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
            ("tarDownload", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ),
            ("write", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
            ("upload", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
            ("uploadDir", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
            ("tarUpload", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE),
            ("delete", NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_DELETE),
            (
                "deleteRecursive",
                NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_DELETE,
            ),
        ] {
            let descriptor = HOST_API_CATALOG
                .iter()
                .find(|descriptor| descriptor.namespace == "sftp" && descriptor.method == method)
                .expect("direct SFTP API must be cataloged");
            assert_eq!(descriptor.capability, Some(capability));
        }

        for method in ["delete", "restart", "update"] {
            let descriptor = HOST_API_CATALOG
                .iter()
                .find(|descriptor| descriptor.namespace == "forward" && descriptor.method == method)
                .expect("forward mutation API must be cataloged");
            assert_eq!(
                descriptor.capability,
                Some(NATIVE_PLUGIN_CAPABILITY_NETWORK_FORWARD)
            );
        }
    }
}
