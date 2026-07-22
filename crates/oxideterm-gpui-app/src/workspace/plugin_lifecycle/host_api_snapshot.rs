// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Workspace sampling for read-only native plugin host API snapshots.
//!
//! `oxideterm-plugin-host-api` owns the DTO and dispatcher. The GPUI app only
//! reads live workspace state that requires `WorkspaceApp` or `Context`.

use std::collections::{BTreeMap, HashMap};

use gpui::Context;
use oxideterm_notification_center::{
    NotificationEntry, NotificationKind, NotificationScope, NotificationSeverity,
    NotificationStatus,
};
use oxideterm_quick_commands::{QuickCommand, QuickCommandCategory};
use oxideterm_theme::{BUILT_IN_THEMES, ThemeTokens};
use serde_json::json;

use super::*;

pub(super) use oxideterm_plugin_host_api::readonly::{
    NativePluginHostApiSnapshot, native_plugin_returnable_host_api_response,
    native_plugin_ui_registration_from_args, native_plugin_ui_tab_id_arg,
};

pub(super) fn native_plugin_host_api_snapshot_from_workspace(
    workspace: &WorkspaceApp,
    cx: &mut Context<WorkspaceApp>,
) -> NativePluginHostApiSnapshot {
    let settings = workspace.settings_store.settings();
    let monitor_stats = workspace.ssh_registry.monitor_stats();
    let mut connection_infos = workspace.ssh_registry.list();
    connection_infos.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));
    let connections = connection_infos
        .iter()
        .map(native_plugin_connection_snapshot)
        .collect::<Vec<_>>();
    let saved_connections = workspace
        .connection_store
        .connection_infos()
        .iter()
        .map(native_plugin_saved_connection_snapshot)
        .collect::<Vec<_>>();
    let connection_states = connection_infos
        .iter()
        .map(|info| {
            (
                info.connection_id.clone(),
                native_plugin_connection_state(&info.state),
            )
        })
        .collect::<HashMap<_, _>>();
    let node_connection_ids = workspace
        .node_runtime_store
        .export_snapshot()
        .nodes
        .into_iter()
        .filter_map(|node| {
            node.connection_id
                .map(|connection_id| (node.id.0, connection_id))
        })
        .collect::<HashMap<_, _>>();
    let session_tree = workspace.native_plugin_session_tree_snapshot_values();
    let session_node_states = native_plugin_session_state_map_from_nodes(&session_tree);
    let event_log_entries =
        native_plugin_event_log_entries(workspace.notification_center.event_log.entries.iter());
    let (active_terminal_target, terminal_nodes) =
        native_plugin_terminal_snapshots(workspace, &connection_states, cx);
    let notification_summary = native_plugin_notification_summary(
        &workspace.notification_center.notifications.entries,
        workspace.notification_center.notifications.unread_count,
        workspace
            .notification_center
            .notifications
            .unread_critical_count,
        workspace.notification_center.notifications.dnd_enabled,
    );
    let notifications =
        native_plugin_notifications_snapshot(&workspace.notification_center.notifications.entries);
    let quick_command_metadata = native_plugin_quick_command_metadata(
        &workspace.quick_commands.categories,
        &workspace.quick_commands.commands,
    );
    let quick_commands = json!({
        "categories": &workspace.quick_commands.categories,
        "commands": &workspace.quick_commands.commands,
    });
    let theme_tokens =
        native_plugin_theme_tokens_snapshot(&workspace.tokens, &settings.terminal.theme);
    let available_themes = native_plugin_available_themes(settings);
    let cloud_sync_summary = native_plugin_cloud_sync_summary(
        workspace.cloud_sync.controller.store.state(),
        workspace.cloud_sync.controller.active_action,
        workspace.cloud_sync.controller.progress.as_ref(),
    );
    let cloud_sync_history = native_plugin_cloud_sync_history(
        &workspace.cloud_sync.controller.store.state().sync_history,
    );
    let host_tools_snapshots =
        oxideterm_plugin_host_api::host_tools::native_plugin_host_tools_snapshot_array(
            &workspace.connection_monitor.profiler_registry,
            &native_plugin_profiler_node_connection_ids(workspace),
        );

    NativePluginHostApiSnapshot {
        registry: workspace.native_plugin_runtime.registry.clone(),
        i18n: workspace.i18n.clone(),
        settings: serde_json::to_value(settings).unwrap_or_else(|_| json!({})),
        locale: settings.general.language.as_str().to_string(),
        theme_name: settings.terminal.theme.clone(),
        // Tauri's PluginAppAPI exposes the compact ssh_get_pool_stats shape,
        // not the full native monitor payload. Keep this RPC-compatible.
        pool_stats: json!({
            "activeConnections": monitor_stats.active_connections,
            "totalSessions": monitor_stats.total_terminals,
        }),
        layout: workspace.native_plugin_layout_snapshot(),
        connections,
        saved_connections,
        connection_states,
        node_connection_ids,
        session_tree,
        session_node_states,
        event_log_entries,
        active_terminal_target,
        terminal_nodes,
        notification_summary,
        notifications,
        quick_command_metadata,
        quick_commands,
        theme_tokens,
        available_themes,
        cloud_sync_summary,
        cloud_sync_history,
        host_tools_snapshots,
    }
}

/// Projects saved connection configuration without credential or local-path fields.
fn native_plugin_saved_connection_snapshot(
    connection: &oxideterm_connections::ConnectionInfo,
) -> Value {
    let proxy_chain = connection
        .proxy_chain
        .iter()
        .map(|hop| {
            json!({
                "host": &hop.host,
                "port": hop.port,
                "username": &hop.username,
                "authType": &hop.auth_type,
                "agentForwarding": hop.agent_forwarding,
                "legacySshCompatibility": hop.legacy_ssh_compatibility,
            })
        })
        .collect::<Vec<_>>();
    let upstream_proxy = match &connection.upstream_proxy {
        oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal => {
            json!({ "mode": "use_global" })
        }
        oxideterm_connections::SavedUpstreamProxyPolicy::Direct => {
            json!({ "mode": "direct" })
        }
        oxideterm_connections::SavedUpstreamProxyPolicy::Custom { proxy } => json!({
            "mode": "custom",
            "protocol": &proxy.protocol,
            "host": &proxy.host,
            "port": proxy.port,
            "hasAuth": !matches!(
                proxy.auth,
                oxideterm_connections::SavedUpstreamProxyAuth::None
            ),
            "remoteDns": proxy.remote_dns,
        }),
    };
    json!({
        "id": &connection.id,
        "name": oxideterm_ai::sanitize_for_ai(&connection.name),
        "group": connection.group.as_deref().map(oxideterm_ai::sanitize_for_ai),
        "host": &connection.host,
        "port": connection.port,
        "username": &connection.username,
        "authType": &connection.auth_type,
        "proxyChain": proxy_chain,
        "upstreamProxy": upstream_proxy,
        "createdAt": &connection.created_at,
        "lastUsedAt": &connection.last_used_at,
        "color": &connection.color,
        "icon": &connection.icon,
        "tags": &connection.tags,
        "agentForwarding": connection.agent_forwarding,
        "legacySshCompatibility": connection.legacy_ssh_compatibility,
    })
}

/// Exposes notification content only through the capability-gated full snapshot.
fn native_plugin_notifications_snapshot(
    entries: &std::collections::VecDeque<NotificationEntry>,
) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|entry| {
                let created_at = entry
                    .created_at
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_millis() as u64)
                    .unwrap_or_default();
                let scope = match &entry.scope {
                    NotificationScope::Global => json!({ "kind": "global" }),
                    NotificationScope::Node(node_id) => {
                        json!({ "kind": "node", "nodeId": node_id })
                    }
                    NotificationScope::Connection(connection_id) => {
                        json!({ "kind": "connection", "connectionId": connection_id })
                    }
                };
                json!({
                    "id": entry.id,
                    "createdAt": created_at,
                    "kind": format!("{:?}", entry.kind).to_ascii_lowercase(),
                    "severity": format!("{:?}", entry.severity).to_ascii_lowercase(),
                    "status": format!("{:?}", entry.status).to_ascii_lowercase(),
                    "title": &entry.title,
                    "body": &entry.body,
                    "scope": scope,
                })
            })
            .collect(),
    )
}

/// Returns discoverable theme identities without exposing custom theme source payloads.
fn native_plugin_available_themes(settings: &oxideterm_settings::PersistedSettings) -> Value {
    let mut custom_ids = settings.custom_themes.keys().cloned().collect::<Vec<_>>();
    custom_ids.sort();
    json!({
        "active": &settings.terminal.theme,
        "builtIn": BUILT_IN_THEMES.iter().map(|theme| theme.id).collect::<Vec<_>>(),
        "custom": custom_ids,
    })
}

/// Removes operational errors and remote revisions from Cloud Sync history.
fn native_plugin_cloud_sync_history(
    history: &[oxideterm_cloud_sync::state::CloudSyncHistoryEntry],
) -> Value {
    Value::Array(
        history
            .iter()
            .map(|entry| {
                json!({
                    "id": &entry.id,
                    "action": &entry.action,
                    "timestamp": &entry.timestamp,
                    "success": entry.success,
                    "summary": &entry.summary,
                })
            })
            .collect(),
    )
}

/// Projects Cloud Sync state through an explicit allowlist so remote locations,
/// credentials, error details, and synchronized content never reach plugins.
fn native_plugin_cloud_sync_summary(
    state: &oxideterm_cloud_sync::state::CloudSyncPersistedState,
    active_action: Option<&str>,
    progress: Option<&oxideterm_cloud_sync::progress::CloudSyncProgress>,
) -> Value {
    let progress = progress.map(|progress| {
        let percent =
            if progress.total.is_finite() && progress.current.is_finite() && progress.total > 0.0 {
                (progress.current / progress.total * 100.0).clamp(0.0, 100.0)
            } else {
                0.0
            };
        json!({
            "stage": native_plugin_cloud_sync_progress_stage(progress.stage),
            "percent": percent,
        })
    });

    json!({
        "autoUploadEnabled": state.settings.auto_upload_enabled,
        "backend": native_plugin_cloud_sync_backend(&state.settings.backend_type),
        "configured": native_plugin_cloud_sync_is_configured(state),
        "status": native_plugin_cloud_sync_status(&state.status),
        "activeAction": active_action,
        "progress": progress,
        "dirty": state.local_dirty,
        "conflict": matches!(state.status, oxideterm_cloud_sync::CloudSyncStatus::Conflict)
            || state.auto_upload_blocked_by_conflict
            || state.conflict_details.is_some(),
        "historyCount": state.sync_history.len(),
        "lastSuccessAt": state.last_sync_at,
    })
}

fn native_plugin_cloud_sync_is_configured(
    state: &oxideterm_cloud_sync::state::CloudSyncPersistedState,
) -> bool {
    use oxideterm_cloud_sync::{AuthMode, BackendType, secret_keys};

    let settings = &state.settings;
    let has_secret = |key: &str| state.secret_hints.get(key).copied().unwrap_or(false);
    let namespace_configured = !settings.namespace.trim().is_empty()
        || matches!(
            settings.backend_type,
            BackendType::S3 | BackendType::Git | BackendType::GithubGist | BackendType::GoogleDrive
        );
    let backend_configured = match settings.backend_type {
        BackendType::Webdav | BackendType::HttpJson => {
            let auth_configured = match settings.auth_mode {
                AuthMode::Bearer => has_secret(secret_keys::TOKEN),
                AuthMode::Basic => {
                    has_secret(secret_keys::BASIC_USERNAME)
                        && has_secret(secret_keys::BASIC_PASSWORD)
                }
                AuthMode::None => true,
            };
            !settings.endpoint.trim().is_empty() && auth_configured
        }
        BackendType::Dropbox => has_secret(secret_keys::TOKEN),
        BackendType::OneDrive => {
            !settings.microsoft_oauth_client_id.trim().is_empty()
                && has_secret(secret_keys::MICROSOFT_REFRESH_TOKEN)
        }
        BackendType::GoogleDrive => {
            !settings.google_oauth_client_id.trim().is_empty()
                && has_secret(secret_keys::GOOGLE_REFRESH_TOKEN)
        }
        BackendType::GithubGist => {
            !settings.git_repository.trim().is_empty() && has_secret(secret_keys::GIT_TOKEN)
        }
        BackendType::S3 => {
            !settings.endpoint.trim().is_empty()
                && !settings.s3_bucket.trim().is_empty()
                && !settings.s3_region.trim().is_empty()
                && has_secret(secret_keys::ACCESS_KEY_ID)
                && has_secret(secret_keys::SECRET_ACCESS_KEY)
        }
        BackendType::Git => {
            !settings.git_repository.trim().is_empty() && has_secret(secret_keys::GIT_TOKEN)
        }
    };

    namespace_configured && backend_configured && has_secret(secret_keys::SYNC_PASSWORD)
}

fn native_plugin_cloud_sync_backend(backend: &oxideterm_cloud_sync::BackendType) -> &'static str {
    use oxideterm_cloud_sync::BackendType;

    match backend {
        BackendType::Webdav => "webdav",
        BackendType::HttpJson => "http-json",
        BackendType::Dropbox => "dropbox",
        BackendType::OneDrive => "one-drive",
        BackendType::GoogleDrive => "google-drive",
        BackendType::GithubGist => "github-gist",
        BackendType::S3 => "s3",
        BackendType::Git => "git",
    }
}

fn native_plugin_cloud_sync_status(status: &oxideterm_cloud_sync::CloudSyncStatus) -> &'static str {
    use oxideterm_cloud_sync::CloudSyncStatus;

    match status {
        CloudSyncStatus::Idle => "idle",
        CloudSyncStatus::Uploading => "uploading",
        CloudSyncStatus::Checking => "checking",
        CloudSyncStatus::RemoteUpdate => "remote-update",
        CloudSyncStatus::Conflict => "conflict",
        CloudSyncStatus::Error => "error",
    }
}

fn native_plugin_cloud_sync_progress_stage(
    stage: oxideterm_cloud_sync::progress::CloudSyncProgressStage,
) -> &'static str {
    use oxideterm_cloud_sync::progress::CloudSyncProgressStage;

    match stage {
        CloudSyncProgressStage::FetchMetadata => "fetch-metadata",
        CloudSyncProgressStage::Preflight => "preflight",
        CloudSyncProgressStage::Exporting => "exporting",
        CloudSyncProgressStage::UploadingBlob => "uploading-blob",
        CloudSyncProgressStage::Downloading => "downloading",
        CloudSyncProgressStage::Validating => "validating",
        CloudSyncProgressStage::PreviewingImport => "previewing-import",
        CloudSyncProgressStage::Importing => "importing",
        CloudSyncProgressStage::CreatingBackup => "creating-backup",
        CloudSyncProgressStage::Done => "done",
        _ => "unknown",
    }
}

/// Projects notification state to counts so default access never reveals user-facing content.
fn native_plugin_notification_summary(
    entries: &std::collections::VecDeque<NotificationEntry>,
    unread_count: u32,
    unread_critical_count: u32,
    dnd_enabled: bool,
) -> Value {
    // Include zero-valued buckets so plugins can consume a stable shape without
    // reconstructing the host's current enum variants.
    let mut by_kind = BTreeMap::from([
        ("agent", 0usize),
        ("connection", 0),
        ("health", 0),
        ("plugin", 0),
        ("security", 0),
        ("transfer", 0),
        ("update", 0),
    ]);
    let mut by_severity = BTreeMap::from([
        ("critical", 0usize),
        ("error", 0),
        ("info", 0),
        ("warning", 0),
    ]);
    let mut by_status = BTreeMap::from([("read", 0usize), ("unread", 0)]);
    for entry in entries {
        let kind = match entry.kind {
            NotificationKind::Connection => "connection",
            NotificationKind::Security => "security",
            NotificationKind::Transfer => "transfer",
            NotificationKind::Update => "update",
            NotificationKind::Health => "health",
            NotificationKind::Plugin => "plugin",
            NotificationKind::Agent => "agent",
        };
        let severity = match entry.severity {
            NotificationSeverity::Info => "info",
            NotificationSeverity::Warning => "warning",
            NotificationSeverity::Error => "error",
            NotificationSeverity::Critical => "critical",
        };
        let status = match entry.status {
            NotificationStatus::Unread => "unread",
            NotificationStatus::Read => "read",
        };
        *by_kind.entry(kind).or_default() += 1;
        *by_severity.entry(severity).or_default() += 1;
        *by_status.entry(status).or_default() += 1;
    }
    json!({
        "total": entries.len(),
        "unread": unread_count,
        "unreadCritical": unread_critical_count,
        "dndEnabled": dnd_enabled,
        "byKind": by_kind,
        "bySeverity": by_severity,
        "byStatus": by_status,
    })
}

/// Exposes command discovery metadata while retaining command text and host patterns in the host.
fn native_plugin_quick_command_metadata(
    categories: &[QuickCommandCategory],
    commands: &[QuickCommand],
) -> Value {
    json!({
        "categories": categories.iter().map(|category| json!({
            "id": category.id,
            "name": category.name,
            "icon": category.icon,
        })).collect::<Vec<_>>(),
        "commands": commands.iter().map(|command| json!({
            "id": command.id,
            "name": command.name,
            "category": command.category,
            "hasDescription": command.description.as_ref().is_some_and(|value| !value.is_empty()),
            "hostRestricted": command.host_pattern.as_ref().is_some_and(|value| !value.is_empty()),
        })).collect::<Vec<_>>(),
    })
}

/// Serializes the effective token object so plugins see the same palette and layout values as GPUI.
fn native_plugin_theme_tokens_snapshot(tokens: &ThemeTokens, theme_name: &str) -> Value {
    let mut value = serde_json::to_value(tokens).unwrap_or_else(|_| json!({}));
    if let Some(object) = value.as_object_mut() {
        object.insert("name".to_string(), json!(theme_name));
    }
    value
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use oxideterm_connections::{
        AuthType, ConnectionInfo, ProxyHopInfo, SavedUpstreamProxyAuth, SavedUpstreamProxyConfig,
        SavedUpstreamProxyPolicy, SavedUpstreamProxyProtocol,
    };
    use oxideterm_notification_center::NotificationScope;
    use oxideterm_quick_commands::QuickCommandIcon;

    use super::*;

    #[test]
    fn saved_connection_snapshot_omits_secret_refs_paths_and_commands() {
        let connection = ConnectionInfo {
            id: "saved-1".to_string(),
            name: "Production".to_string(),
            group: Some("Servers".to_string()),
            host: "example.test".to_string(),
            port: 22,
            username: "operator".to_string(),
            auth_type: AuthType::Key,
            key_path: Some("/private/id_ed25519".to_string()),
            cert_path: Some("/private/id_ed25519-cert.pub".to_string()),
            managed_key_id: Some("managed-secret-ref".to_string()),
            managed_key_name: Some("Managed key".to_string()),
            proxy_chain: vec![ProxyHopInfo {
                host: "jump.example.test".to_string(),
                port: 22,
                username: "jump-user".to_string(),
                auth_type: AuthType::Key,
                key_path: Some("/private/jump-key".to_string()),
                cert_path: None,
                managed_key_id: Some("jump-secret-ref".to_string()),
                managed_key_name: None,
                agent_forwarding: false,
                legacy_ssh_compatibility: false,
            }],
            upstream_proxy: SavedUpstreamProxyPolicy::Custom {
                proxy: SavedUpstreamProxyConfig {
                    protocol: SavedUpstreamProxyProtocol::Socks5,
                    host: "proxy.example.test".to_string(),
                    port: 1080,
                    auth: SavedUpstreamProxyAuth::Password {
                        username: "proxy-user".to_string(),
                        keychain_id: Some("proxy-secret-ref".to_string()),
                        plaintext_password: None,
                    },
                    remote_dns: true,
                    no_proxy: "internal.example.test".to_string(),
                },
            },
            created_at: "2026-07-22T00:00:00Z".to_string(),
            last_used_at: None,
            color: None,
            icon: None,
            tags: vec!["production".to_string()],
            agent_forwarding: false,
            legacy_ssh_compatibility: false,
            post_connect_command: Some("export TOKEN=private".to_string()),
        };

        let serialized = native_plugin_saved_connection_snapshot(&connection).to_string();
        for private_value in [
            "/private/id_ed25519",
            "/private/id_ed25519-cert.pub",
            "managed-secret-ref",
            "/private/jump-key",
            "jump-secret-ref",
            "proxy-secret-ref",
            "internal.example.test",
            "export TOKEN=private",
        ] {
            assert!(!serialized.contains(private_value));
        }
        assert!(serialized.contains("jump.example.test"));
        assert!(serialized.contains("proxy.example.test"));
        assert!(serialized.contains("\"hasAuth\":true"));
    }

    #[test]
    fn notification_summary_counts_entries_without_exposing_content() {
        let entries = VecDeque::from([NotificationEntry {
            id: 1,
            created_at: std::time::UNIX_EPOCH,
            kind: NotificationKind::Security,
            severity: NotificationSeverity::Critical,
            title: "private notification title".to_string(),
            body: Some("private notification body".to_string()),
            status: NotificationStatus::Unread,
            scope: NotificationScope::Node("private-node".to_string()),
            dedupe_key: Some("private-dedupe".to_string()),
        }]);

        let summary = native_plugin_notification_summary(&entries, 1, 1, false);
        assert_eq!(summary["total"], 1);
        assert_eq!(summary["byKind"]["security"], 1);
        assert_eq!(summary["byKind"]["connection"], 0);
        assert_eq!(summary["bySeverity"]["critical"], 1);
        assert_eq!(summary["byStatus"]["unread"], 1);
        let serialized = summary.to_string();
        for private_value in [
            "private notification title",
            "private notification body",
            "private-node",
            "private-dedupe",
        ] {
            assert!(!serialized.contains(private_value));
        }
    }

    #[test]
    fn quick_command_metadata_retains_discovery_fields_without_executable_content() {
        let categories = vec![QuickCommandCategory {
            id: "ops".to_string(),
            name: "Operations".to_string(),
            icon: QuickCommandIcon::Server,
        }];
        let commands = vec![QuickCommand {
            id: "restart".to_string(),
            name: "Restart service".to_string(),
            command: "private executable command".to_string(),
            category: "ops".to_string(),
            description: Some("Restarts the service".to_string()),
            host_pattern: Some("private-host-*".to_string()),
            created_at: 1,
            updated_at: 2,
        }];

        let metadata = native_plugin_quick_command_metadata(&categories, &commands);
        assert_eq!(metadata["categories"][0]["icon"], "server");
        assert_eq!(metadata["commands"][0]["hasDescription"], true);
        assert_eq!(metadata["commands"][0]["hostRestricted"], true);
        let command = &metadata["commands"][0];
        assert!(command.get("command").is_none());
        assert!(command.get("description").is_none());
        assert!(command.get("hostPattern").is_none());
        let serialized = metadata.to_string();
        assert!(!serialized.contains("private executable command"));
        assert!(!serialized.contains("private-host-*"));
    }

    #[test]
    fn theme_token_snapshot_contains_every_stable_token_group() {
        let tokens = oxideterm_theme::default_tokens();
        let snapshot = native_plugin_theme_tokens_snapshot(&tokens, "default");

        assert_eq!(snapshot["name"], "default");
        for group in [
            "terminal", "ui", "metrics", "radii", "spacing", "density", "motion",
        ] {
            assert!(snapshot.get(group).is_some(), "missing theme group {group}");
        }
        assert_eq!(
            snapshot["terminal"]["background"],
            tokens.terminal.background
        );
        assert_eq!(
            snapshot["metrics"]["titlebarHeight"],
            tokens.metrics.titlebar_height
        );
        assert_eq!(snapshot["density"], "comfortable");
    }

    #[test]
    fn cloud_sync_summary_exposes_status_without_private_configuration_or_content() {
        use oxideterm_cloud_sync::{
            AuthMode, CloudSyncStatus,
            progress::{CloudSyncProgress, CloudSyncProgressStage},
            secret_keys,
            state::{
                CloudSyncConflictDetails, CloudSyncHistoryEntry, CloudSyncHistorySummary,
                CloudSyncPersistedState, CloudSyncRollbackBackup,
            },
        };

        let mut state = CloudSyncPersistedState::default();
        state.settings.auth_mode = AuthMode::None;
        state.settings.auto_upload_enabled = true;
        state.settings.endpoint = "https://private-sync.example.test/secret-path".to_string();
        state.settings.namespace = "private-account/private-bucket".to_string();
        state
            .secret_hints
            .insert(secret_keys::SYNC_PASSWORD.to_string(), true);
        state.status = CloudSyncStatus::Conflict;
        state.local_dirty = true;
        state.last_sync_at = Some("2026-07-22T08:00:00Z".to_string());
        state.last_error = Some("private error detail with bearer-secret".to_string());
        state.last_known_remote_revision = Some("private-remote-revision".to_string());
        state.remote_device_id = Some("private-account-id".to_string());
        state.conflict_details = Some(CloudSyncConflictDetails {
            revision: Some("private-conflict-revision".to_string()),
            device_id: Some("private-conflict-device".to_string()),
            updated_at: Some("private-conflict-time".to_string()),
        });
        state.sync_history.push(CloudSyncHistoryEntry::new(
            "private-action-detail",
            CloudSyncHistorySummary::default(),
            false,
            Some("private-history-error".to_string()),
            Some("private-history-revision".to_string()),
        ));
        state.rollback_backups.push(CloudSyncRollbackBackup {
            id: "private-backup-id".to_string(),
            created_at: "private-backup-time".to_string(),
            source_revision: Some("private-backup-revision".to_string()),
            size_bytes: 12,
            bytes_base64: "private-snapshot-content".to_string(),
            metadata: None,
        });
        let progress = CloudSyncProgress {
            stage: CloudSyncProgressStage::UploadingBlob,
            current: 1.0,
            total: 4.0,
            message: Some("private-progress-message".to_string()),
        };

        let summary = native_plugin_cloud_sync_summary(&state, Some("upload"), Some(&progress));
        assert_eq!(summary["autoUploadEnabled"], true);
        assert_eq!(summary["configured"], true);
        assert_eq!(summary["status"], "conflict");
        assert_eq!(summary["progress"]["stage"], "uploading-blob");
        assert_eq!(summary["progress"]["percent"], 25.0);
        assert_eq!(summary["historyCount"], 1);
        assert_eq!(summary["lastSuccessAt"], "2026-07-22T08:00:00Z");

        let serialized = summary.to_string();
        for private_value in [
            "private-sync.example.test",
            "private-account/private-bucket",
            "bearer-secret",
            "private-remote-revision",
            "private-account-id",
            "private-conflict-revision",
            "private-history-error",
            "private-backup-id",
            "private-snapshot-content",
            "private-progress-message",
        ] {
            assert!(!serialized.contains(private_value));
        }
        for forbidden_key in [
            "endpoint",
            "namespace",
            "bucket",
            "account",
            "token",
            "password",
            "header",
            "error",
            "snapshot",
            "message",
        ] {
            assert!(!serialized.to_ascii_lowercase().contains(forbidden_key));
        }
    }
}
