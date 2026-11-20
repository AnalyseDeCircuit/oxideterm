use serde_json::Value;

use super::{
    AiActionResultLite, AiOrchestratorRuntimeSnapshot, AiOrchestratorTarget,
    AiStableResourceOperation, ai_application_action_result, ai_memory_trimmed_content,
    ai_stable_resource_operation,
};

/// Workspace operations requested after authority validation. Their owners receive
/// the original arguments; only this module decides the model-visible result.
pub enum AiApplicationTool {
    ConnectTarget,
    ObserveTerminal,
    SendTerminalInput,
    GetTerminalCommandStatus,
    ReadResource,
    WriteResource,
    OpenAppSurface,
    GetState,
    InspectHostTools,
    ControlHostTool,
    ListForwards,
    ManageForward,
    ListPlugins,
    ManagePlugin,
    ListTransportProfiles,
    OpenTransportProfile,
    GetTransportSessionState,
    ManageSerialSession,
    ManageTelnetSession,
    ListRemoteDesktopSessions,
    ManageRemoteDesktopSession,
    GetCloudSyncState,
    ConfigureCloudSync,
    ManageCloudSync,
    ListCredentials,
    ManageCredential,
    ListMemoryEntries,
    ManageMemoryEntry,
    CreateBackgroundTask,
    ListBackgroundTasks,
    GetBackgroundTask,
    CancelBackgroundTask,
    RememberPreference,
    LoadSkill,
    ReadSkillResource,
}

pub fn dispatch_ai_application_tool(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    tool_name: &str,
    args: &Value,
    invoke: impl FnOnce(AiApplicationTool) -> AiActionResultLite,
) -> AiActionResultLite {
    use AiApplicationTool::*;
    let request = match tool_name {
        "list_targets" => return snapshot.list_targets(args),
        "select_target" => return snapshot.select_target(args),
        "run_command" => {
            return snapshot.fail(
                "Command execution requires the asynchronous runtime broker.",
                "runtime_capability_unavailable",
                "Retry the command through the current tool session.",
                "interactive",
            );
        }
        "wait_terminal_output" => {
            return snapshot.fail(
                "Terminal waiting requires the asynchronous runtime broker.",
                "runtime_capability_unavailable",
                "Retry the wait through the current tool session.",
                "read",
            );
        }
        "transfer_resource" => {
            return snapshot.fail(
                "SFTP capability is unavailable.",
                "runtime_capability_unavailable",
                "Rediscover current resources after the SFTP capability owner is available.",
                "write",
            );
        }
        "recall_preferences" => {
            return {
                let memory_content = ai_memory_trimmed_content(&snapshot.memory);
                snapshot.ok(
                    if memory_content.is_empty() {
                        "No saved preferences."
                    } else {
                        "Preferences recalled."
                    },
                    if memory_content.is_empty() {
                        "No saved preferences.".to_string()
                    } else {
                        memory_content.to_string()
                    },
                    snapshot.memory.clone(),
                    "read",
                )
            };
        }
        "connect_target" => ConnectTarget,
        "observe_terminal" => ObserveTerminal,
        "send_terminal_input" => SendTerminalInput,
        "get_terminal_command_status" => GetTerminalCommandStatus,
        "read_resource" => ReadResource,
        "write_resource" => WriteResource,
        "open_app_surface" => OpenAppSurface,
        "get_state" => GetState,
        "inspect_host_tools" => InspectHostTools,
        "control_host_tool" => ControlHostTool,
        "list_forwards" => ListForwards,
        "manage_forward" => ManageForward,
        "list_plugins" => ListPlugins,
        "manage_plugin" => ManagePlugin,
        "list_transport_profiles" => ListTransportProfiles,
        "open_transport_profile" => OpenTransportProfile,
        "get_transport_session_state" => GetTransportSessionState,
        "manage_serial_session" => ManageSerialSession,
        "manage_telnet_session" => ManageTelnetSession,
        "list_remote_desktop_sessions" => ListRemoteDesktopSessions,
        "manage_remote_desktop_session" => ManageRemoteDesktopSession,
        "get_cloud_sync_state" => GetCloudSyncState,
        "configure_cloud_sync" => ConfigureCloudSync,
        "manage_cloud_sync" => ManageCloudSync,
        "list_credentials" => ListCredentials,
        "manage_credential" => ManageCredential,
        "list_memory_entries" => ListMemoryEntries,
        "manage_memory_entry" => ManageMemoryEntry,
        "create_background_task" => CreateBackgroundTask,
        "list_background_tasks" => ListBackgroundTasks,
        "get_background_task" => GetBackgroundTask,
        "cancel_background_task" => CancelBackgroundTask,
        "remember_preference" => RememberPreference,
        "load_skill" => LoadSkill,
        "read_skill_resource" => ReadSkillResource,
        _ => {
            return snapshot.fail(
                "Unknown orchestrator tool.",
                "unknown_tool",
                format!("{tool_name} is not an OxideSens task tool."),
                "read",
            );
        }
    };
    invoke(request)
}

impl AiApplicationTool {
    pub fn result(
        self,
        snapshot: &AiOrchestratorRuntimeSnapshot,
        result: Result<Value, String>,
    ) -> AiActionResultLite {
        use AiApplicationTool::*;
        let (summary, risk) = match self {
            InspectHostTools => ("Host Tools inspection completed.", "read"),
            ControlHostTool => ("Host Tools action accepted.", "execute"),
            ListForwards => ("Forwarding rules listed.", "read"),
            ManageForward => ("Forwarding action accepted.", "write"),
            ListPlugins => ("Installed plugins listed.", "read"),
            ManagePlugin => ("Plugin action accepted.", "write"),
            ListTransportProfiles => ("Saved transport profiles listed.", "read"),
            OpenTransportProfile => ("Transport profile open request accepted.", "interactive"),
            GetTransportSessionState => ("Transport session state read.", "read"),
            ManageSerialSession => ("Serial session action completed.", "interactive"),
            ManageTelnetSession => ("Telnet session action completed.", "interactive"),
            ListRemoteDesktopSessions => ("Remote desktop sessions listed.", "read"),
            ManageRemoteDesktopSession => {
                ("Remote desktop session action accepted.", "interactive")
            }
            GetCloudSyncState => ("Cloud Sync state inspected.", "read"),
            ConfigureCloudSync => ("Cloud Sync configuration updated.", "write"),
            ManageCloudSync => ("Cloud Sync action accepted.", "write"),
            ListCredentials => ("Credential metadata listed.", "read"),
            ManageCredential => ("Credential management action accepted.", "write"),
            ListMemoryEntries => ("Memory entries listed.", "read"),
            ManageMemoryEntry => ("Memory entry action completed.", "write"),
            CreateBackgroundTask => ("Background task created.", "write"),
            ListBackgroundTasks => ("Background tasks listed.", "read"),
            GetBackgroundTask => ("Background task inspected.", "read"),
            CancelBackgroundTask => ("Background task cancelled.", "write"),
            _ => unreachable!("this tool already returns its domain result"),
        };
        ai_application_action_result(snapshot, result, summary, risk)
    }
}

pub fn ai_read_stable_resource(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    args: &Value,
    rag_store: impl FnOnce() -> std::sync::Arc<crate::RagStore>,
) -> AiActionResultLite {
    match ai_stable_resource_operation("read_resource", args) {
        Ok(AiStableResourceOperation::Settings) => {
            let Some(target) = snapshot
                .targets
                .iter()
                .find(|target| target.kind == "settings")
                .cloned()
            else {
                return snapshot.fail(
                    "Settings resource is unavailable.",
                    "resource_not_found",
                    "Rediscover the application settings resource before reading it.",
                    "read",
                );
            };
            let section = args.get("section").and_then(serde_json::Value::as_str);
            let data = section
                .and_then(|section| snapshot.model_visible_settings.get(section).cloned())
                .unwrap_or_else(|| snapshot.model_visible_settings.clone());
            snapshot
                .ok(
                    section
                        .map(|section| format!("Read settings section {section}."))
                        .unwrap_or_else(|| "Read settings.".to_string()),
                    serde_json::to_string_pretty(&data).unwrap_or_default(),
                    data,
                    "read",
                )
                .with_target(target)
        }
        Ok(AiStableResourceOperation::Rag) => {
            let Some(target) = snapshot
                .targets
                .iter()
                .find(|target| target.kind == "rag-index")
                .cloned()
            else {
                return snapshot.fail(
                    "Knowledge resource is unavailable.",
                    "resource_not_found",
                    "Rediscover the knowledge resource before searching it.",
                    "read",
                );
            };
            let rag_store = rag_store();
            let results = crate::rag_search(
                &rag_store,
                crate::RagSearchRequest {
                    query: crate::chat_tools::ai_rag_query_arg(args).to_string(),
                    collection_ids: Vec::new(),
                    query_vector: None,
                    top_k: Some(8),
                },
            );
            match results {
                Ok(results) => {
                    let data =
                        serde_json::to_value(results).unwrap_or_else(|_| serde_json::json!([]));
                    snapshot
                        .ok(
                            format!(
                                "Found {} knowledge results.",
                                data.as_array().map(Vec::len).unwrap_or(0)
                            ),
                            serde_json::to_string_pretty(&data).unwrap_or_default(),
                            data,
                            "read",
                        )
                        .with_target(target)
                }
                Err(error) => snapshot
                    .fail(
                        "Knowledge search failed.",
                        "rag_search_error",
                        error,
                        "read",
                    )
                    .with_target(target),
            }
        }
        _ => snapshot.fail(
            "Resource is unavailable.",
            "resource_not_found",
            "Rediscover the resource through the current v2 runtime context.",
            "read",
        ),
    }
}

pub fn ai_list_transport_profiles(
    serial: &[oxideterm_connections::SerialProfile],
    telnet: &[oxideterm_connections::TelnetProfile],
    remote_desktop: &[oxideterm_connections::RemoteDesktopProfile],
) -> Value {
    let serial = serial
        .iter()
        .map(|profile| {
            serde_json::json!({
                "id": profile.id,
                "name": profile.name,
                "group": profile.group,
                "portPath": profile.port_path,
                "baudRate": profile.baud_rate,
                "dataBits": profile.data_bits,
                "stopBits": profile.stop_bits,
                "parity": format!("{:?}", profile.parity).to_lowercase(),
                "flowControl": format!("{:?}", profile.flow_control).to_lowercase(),
            })
        })
        .collect::<Vec<_>>();
    let telnet = telnet
        .iter()
        .map(|profile| {
            serde_json::json!({
                "id": profile.id,
                "name": profile.name,
                "group": profile.group,
                "host": profile.host,
                "port": profile.port,
            })
        })
        .collect::<Vec<_>>();
    let remote_desktop = remote_desktop
        .iter()
        .map(|profile| {
            serde_json::json!({
                "id": profile.id,
                "name": profile.name,
                "group": profile.group,
                "protocol": format!("{:?}", profile.protocol).to_lowercase(),
                "host": profile.host,
                "port": profile.port,
                "username": profile.username,
                "readOnly": profile.read_only,
                "hasCredential": profile.has_configured_password(),
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "serial": serial,
        "telnet": telnet,
        "remoteDesktop": remote_desktop,
    })
}

pub fn ai_list_credential_metadata(
    managed_ssh_keys: Vec<oxideterm_connections::ManagedSshKeyInfo>,
    privilege_credentials: Vec<oxideterm_connections::SavedPrivilegeCredential>,
    remote_desktop: &[oxideterm_connections::RemoteDesktopProfile],
) -> Value {
    let managed_ssh_keys = managed_ssh_keys
        .into_iter()
        .map(|key| {
            serde_json::json!({
                "id": key.id,
                "name": key.name,
                "fingerprint": key.fingerprint,
                "requiresPassphrase": key.requires_passphrase,
                "origin": format!("{:?}", key.origin).to_lowercase(),
                "createdAt": key.created_at,
                "updatedAt": key.updated_at,
            })
        })
        .collect::<Vec<_>>();
    let privilege = privilege_credentials
        .into_iter()
        .map(|credential| {
            serde_json::json!({
                "id": credential.id,
                "connectionId": credential.connection_id,
                "label": credential.label,
                "promptType": credential.prompt_type,
                "responseMode": credential.response_mode,
                "usernameHint": credential.username_hint,
                "hostPatterns": credential.host_patterns,
                "secondFactorPatterns": credential.second_factor_patterns,
                "secondFactorUserHint": credential.second_factor_user_hint,
                "totpCredentialId": credential.totp_credential_id,
                "enabled": credential.enabled,
                "requireClickToSend": credential.require_click_to_send,
                "hasSecret": credential.keychain_id.is_some(),
                "createdAt": credential.created_at.to_rfc3339(),
                "updatedAt": credential.updated_at.to_rfc3339(),
            })
        })
        .collect::<Vec<_>>();
    let remote_desktop = remote_desktop
        .iter()
        .filter(|profile| profile.has_configured_password())
        .map(|profile| {
            serde_json::json!({
                "profileId": profile.id,
                "profileName": profile.name,
                "protocol": format!("{:?}", profile.protocol).to_lowercase(),
                "hasCredential": true,
            })
        })
        .collect::<Vec<_>>();
    serde_json::json!({
        "managedSshKeys": managed_ssh_keys,
        "privilegeCredentials": privilege,
        "remoteDesktopCredentials": remote_desktop,
    })
}

pub fn ai_credential_scopes<'a>(
    args: &Value,
    connection_ids: impl Iterator<Item = &'a str>,
) -> Vec<String> {
    match args.get("connection_id").and_then(Value::as_str) {
        Some(scope) => vec![scope.to_owned()],
        None => std::iter::once(oxideterm_connections::LOCAL_SHELL_PRIVILEGE_CONNECTION_ID)
            .chain(connection_ids)
            .map(str::to_owned)
            .collect(),
    }
}

pub enum AiPluginAction<'a> {
    OpenManager,
    Install {
        package_url: zeroize::Zeroizing<String>,
        checksum: Option<String>,
        overwrite: bool,
    },
    Enable {
        plugin_id: &'a str,
        enabled: bool,
    },
    Uninstall {
        plugin_id: &'a str,
        remove_storage: bool,
    },
    Invoke {
        plugin_id: &'a str,
        command_id: &'a str,
        arguments: Value,
    },
    Configure {
        plugin_id: &'a str,
        setting_id: &'a str,
        value: Value,
    },
}

pub fn ai_plugin_action(arguments: &Value) -> Result<(&str, AiPluginAction<'_>), String> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or("A plugin action is required.")?;
    if action == "open_manager" {
        return Ok((action, AiPluginAction::OpenManager));
    }
    if action == "install" {
        let package_url = arguments
            .get("package_url")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| "A plugin package URL is required.".to_string())?;
        let parsed_url =
            reqwest::Url::parse(package_url).map_err(|_| "The plugin package URL is invalid.")?;
        if !matches!(parsed_url.scheme(), "http" | "https")
            || !parsed_url.username().is_empty()
            || parsed_url.password().is_some()
            || parsed_url.query().is_some()
            || parsed_url.fragment().is_some()
        {
            return Err(
                    "Plugin package URLs must use HTTP(S) without credentials, query strings, or fragments."
                        .to_string(),
                );
        }
        let checksum = arguments
            .get("checksum")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        let overwrite = arguments
            .get("overwrite")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        return Ok((
            action,
            AiPluginAction::Install {
                package_url: zeroize::Zeroizing::new(package_url.to_owned()),
                checksum,
                overwrite,
            },
        ));
    }
    let plugin_id = arguments
        .get("plugin_id")
        .and_then(Value::as_str)
        .ok_or("A plugin identifier is required.")?;
    let operation = match action {
        "enable" | "disable" => AiPluginAction::Enable {
            plugin_id,
            enabled: action == "enable",
        },
        "uninstall" => AiPluginAction::Uninstall {
            plugin_id,
            remove_storage: arguments
                .get("remove_storage")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "invoke" => AiPluginAction::Invoke {
            plugin_id,
            command_id: arguments
                .get("command_id")
                .and_then(Value::as_str)
                .ok_or("A plugin command identifier is required.")?,
            arguments: arguments
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        },
        "configure" => AiPluginAction::Configure {
            plugin_id,
            setting_id: arguments
                .get("setting_id")
                .and_then(Value::as_str)
                .ok_or("A plugin setting identifier is required.")?,
            value: arguments
                .get("value")
                .cloned()
                .ok_or("A plugin setting value is required.")?,
        },
        _ => return Err("Unsupported plugin action.".to_owned()),
    };
    Ok((action, operation))
}

impl<'a> AiPluginAction<'a> {
    pub fn plugin_id(&self) -> Option<&'a str> {
        match self {
            Self::Enable { plugin_id, .. }
            | Self::Uninstall { plugin_id, .. }
            | Self::Invoke { plugin_id, .. }
            | Self::Configure { plugin_id, .. } => Some(plugin_id),
            Self::OpenManager | Self::Install { .. } => None,
        }
    }
}

pub fn ai_plugin_action_accepted(action: &str, plugin_id: Option<&str>) -> Value {
    let mut result = serde_json::json!({ "accepted": true, "action": action });
    if let Some(id) = plugin_id {
        result["pluginId"] = serde_json::json!(id);
    }
    result
}

use super::{ai_target_from_projection, ai_target_projection};

pub enum AiAppSurface<N> {
    LocalTerminal(String),
    Settings,
    ConnectionManager,
    Runtime,
    HostTools,
    FileManager,
    Sftp(N),
    Ide(N),
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn plan_open_surface<N>(
        &self,
        args: &Value,
        active_node: Option<N>,
    ) -> Result<AiAppSurface<N>, AiActionResultLite> {
        let Ok(AiStableResourceOperation::AppSurface(surface)) =
            ai_stable_resource_operation("open_app_surface", args)
        else {
            return Err(self.fail(
                "Application surface is unavailable.",
                "resource_not_found",
                "Rediscover the application surface before opening it.",
                "write",
            ));
        };
        match (surface.as_str(), active_node) {
            ("local_terminal" | "terminal", _) => Ok(AiAppSurface::LocalTerminal(surface)),
            ("settings", _) => Ok(AiAppSurface::Settings),
            ("connection_manager", _) => Ok(AiAppSurface::ConnectionManager),
            ("connection_pool", _) => Ok(AiAppSurface::Runtime),
            ("connection_monitor", _) => Ok(AiAppSurface::HostTools),
            ("file_manager", _) => Ok(AiAppSurface::FileManager),
            ("sftp", Some(node)) => Ok(AiAppSurface::Sftp(node)),
            ("ide", Some(node)) => Ok(AiAppSurface::Ide(node)),
            ("sftp" | "ide", None) => {
                let label = if surface == "sftp" { "SFTP" } else { "IDE" };
                Err(self.fail(format!("{label} requires a connected SSH target."), "missing_node_context",
                    format!("Connect an SSH target first, then rediscover the current {label} surface."),
                    "write").with_next_actions(vec![serde_json::json!({
                        "action": "list_targets", "args": { "view": "files" },
                        "reason": format!("Find a connected {label} or SSH target before opening {label}.")
                    })]))
            }
            _ => Err(self.fail(
                "Unknown app surface.",
                "unknown_app_surface",
                format!("Unknown app surface: {surface}"),
                "write",
            )),
        }
    }

    pub fn opened_surface<N>(
        &self,
        surface: AiAppSurface<N>,
        active_tab_id: Option<oxideterm_workspace::TabId>,
    ) -> AiActionResultLite {
        let summary = match &surface {
            AiAppSurface::LocalTerminal(_) => "Opened local terminal.",
            AiAppSurface::Settings => "Opened settings.",
            AiAppSurface::ConnectionManager => "Opened connection_manager.",
            AiAppSurface::Runtime => "Opened runtime overview.",
            AiAppSurface::HostTools => "Opened Host Tools.",
            AiAppSurface::FileManager => "Opened file_manager.",
            AiAppSurface::Sftp(_) => "Opened sftp.",
            AiAppSurface::Ide(_) => "Opened ide.",
        };
        let (data, target) = match surface {
            AiAppSurface::LocalTerminal(surface) => {
                let tab_id = active_tab_id.map(|id| id.0.to_string());
                let target = self
                    .targets
                    .iter()
                    .find(|target| {
                        target.kind == "terminal-session"
                            && tab_id
                                .as_ref()
                                .is_some_and(|id| target.refs.get("tabId") == Some(id))
                            && target.metadata.get("terminalType").and_then(Value::as_str)
                                == Some("local_terminal")
                    })
                    .map(ai_opened_local_terminal_target);
                (serde_json::json!({ "surface": surface }), target)
            }
            _ => (Value::Null, None),
        };
        self.ok(summary, summary, data, "write")
            .with_optional_target(target)
    }
}

fn ai_opened_local_terminal_target(target: &AiOrchestratorTarget) -> AiOrchestratorTarget {
    // Tauri returns a synthetic local-terminal target from open_app_surface,
    // not the richer target-discovery snapshot that carries tab metadata.
    ai_target_from_projection(crate::opened_local_terminal_projection(
        &ai_target_projection(target),
    ))
}

pub enum AiForwardOperation {
    Create {
        rule: oxideterm_forwarding::ForwardRule,
    },
    Update {
        forward_id: String,
        update: oxideterm_forwarding::ForwardUpdate,
    },
    Stop {
        forward_id: String,
    },
    Restart {
        forward_id: String,
    },
    Delete {
        forward_id: String,
    },
    Scan,
}

pub fn ai_forwarding_operation(
    arguments: &serde_json::Value,
) -> Result<AiForwardOperation, String> {
    let action = arguments
        .get("action")
        .and_then(serde_json::Value::as_str)
        .ok_or("A forwarding action is required.")?;
    let forward_id = || {
        arguments
            .get("forward_id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| "A forwarding rule identifier is required.".to_string())
    };
    match action {
        "create" => {
            let rule = arguments
                .get("rule")
                .cloned()
                .and_then(|value| {
                    serde_json::from_value::<oxideterm_forwarding::ForwardRule>(value).ok()
                })
                .ok_or_else(|| "The forwarding rule is invalid.".to_string())?;
            Ok(AiForwardOperation::Create { rule })
        }
        "update" => {
            let rule = arguments
                .get("rule")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| "The forwarding update is invalid.".to_string())?;
            let update = oxideterm_forwarding::ForwardUpdate {
                forward_type: rule
                    .get("forwardType")
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok()),
                bind_address: rule
                    .get("bindAddress")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                bind_port: rule
                    .get("bindPort")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|port| u16::try_from(port).ok()),
                target_host: rule
                    .get("targetHost")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
                target_port: rule
                    .get("targetPort")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|port| u16::try_from(port).ok()),
                description: rule
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string),
            };
            Ok(AiForwardOperation::Update {
                forward_id: forward_id()?,
                update,
            })
        }
        "stop" => Ok(AiForwardOperation::Stop {
            forward_id: forward_id()?,
        }),
        "restart" => Ok(AiForwardOperation::Restart {
            forward_id: forward_id()?,
        }),
        "delete" => Ok(AiForwardOperation::Delete {
            forward_id: forward_id()?,
        }),
        "scan_ports" => Ok(AiForwardOperation::Scan),
        _ => Err("Unsupported forwarding action.".to_string()),
    }
}

impl AiForwardOperation {
    pub fn action(&self) -> &'static str {
        match self {
            Self::Create { .. } => "create",
            Self::Update { .. } => "update",
            Self::Stop { .. } => "stop",
            Self::Restart { .. } => "restart",
            Self::Delete { .. } => "delete",
            Self::Scan => "scan_ports",
        }
    }
}

pub fn ai_action_accepted(action: &str) -> Value {
    serde_json::json!({ "accepted": true, "action": action })
}

pub enum AiCredentialAction<'a> {
    Open(Option<&'a str>),
    Delete {
        action: &'a str,
        id: &'a str,
        credential: AiCredentialKind<'a>,
    },
}

pub enum AiCredentialKind<'a> {
    ManagedSshKey { force: bool },
    Privilege { connection_id: &'a str },
    RemoteDesktop,
}

impl AiCredentialKind<'_> {
    pub fn result(&self, action: &str, id: &str, deleted: bool) -> Value {
        let kind = match self {
            Self::ManagedSshKey { .. } => "managed_ssh_key",
            Self::Privilege { .. } => "privilege",
            Self::RemoteDesktop => "remote_desktop",
        };
        serde_json::json!({ "accepted": deleted, "action": action, "kind": kind, "id": id })
    }
}

pub fn ai_credential_action(arguments: &Value) -> Result<AiCredentialAction<'_>, String> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "A credential action is required.".to_owned())?;
    let connection_id = arguments.get("connection_id").and_then(Value::as_str);
    if action == "open_manager" {
        return Ok(AiCredentialAction::Open(connection_id));
    }
    let kind = arguments
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "A credential kind is required.".to_owned())?;
    let id = arguments
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "A credential identifier is required.".to_owned())?;
    let credential = match kind {
        "managed_ssh_key" => AiCredentialKind::ManagedSshKey {
            force: arguments
                .get("force")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
        "privilege" => AiCredentialKind::Privilege {
            connection_id: connection_id
                .ok_or_else(|| "A privilege credential scope is required.".to_owned())?,
        },
        "remote_desktop" => AiCredentialKind::RemoteDesktop,
        _ => return Err("Unsupported credential kind.".to_owned()),
    };
    Ok(AiCredentialAction::Delete {
        action,
        id,
        credential,
    })
}

pub enum AiSerialControl {
    RefreshPort,
    SendBreak,
    SetDtr(bool),
    SetRts(bool),
    SetLocalEcho(bool),
    SetLineEnding(oxideterm_connections::SerialLineEnding),
    SetOutputLineEnding(oxideterm_connections::SerialLineEnding),
    SetDisplayMode(oxideterm_connections::SerialDisplayMode),
    SetSendMode(oxideterm_connections::SerialSendMode),
}

pub fn ai_serial_control(arguments: &Value) -> Result<(&str, AiSerialControl), String> {
    use AiSerialControl::*;
    use oxideterm_connections::{SerialDisplayMode, SerialLineEnding, SerialSendMode};
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "A serial action is required.".to_owned())?;
    let enabled = arguments.get("enabled").and_then(Value::as_bool);
    let value = arguments.get("value").and_then(Value::as_str);
    let line_ending = || match value {
        Some("none") => Some(SerialLineEnding::None),
        Some("lf") => Some(SerialLineEnding::Lf),
        Some("crlf") => Some(SerialLineEnding::CrLf),
        Some("cr") => Some(SerialLineEnding::Cr),
        _ => None,
    };
    let control = match action {
        "refresh_port" => RefreshPort,
        "reconnect" => {
            return Err(
                "Serial reconnect must create a new terminal tab from Active Sessions.".to_owned(),
            );
        }
        "send_break" => SendBreak,
        "set_dtr" => SetDtr(enabled.ok_or_else(|| "The DTR state is required.".to_owned())?),
        "set_rts" => SetRts(enabled.ok_or_else(|| "The RTS state is required.".to_owned())?),
        "set_local_echo" => {
            SetLocalEcho(enabled.ok_or_else(|| "The local echo state is required.".to_owned())?)
        }
        "set_line_ending" => SetLineEnding(
            line_ending().ok_or_else(|| "A valid serial line ending is required.".to_owned())?,
        ),
        "set_output_line_ending" => SetOutputLineEnding(
            line_ending()
                .ok_or_else(|| "A valid serial output line ending is required.".to_owned())?,
        ),
        "set_display_mode" => SetDisplayMode(match value {
            Some("text") => SerialDisplayMode::Text,
            Some("hex") => SerialDisplayMode::Hex,
            Some("mixed") => SerialDisplayMode::Mixed,
            _ => return Err("A valid serial display mode is required.".to_owned()),
        }),
        "set_send_mode" => SetSendMode(match value {
            Some("text") => SerialSendMode::Text,
            Some("hex") => SerialSendMode::Hex,
            _ => return Err("A valid serial send mode is required.".to_owned()),
        }),
        _ => return Err("Unsupported serial action.".to_owned()),
    };
    Ok((action, control))
}

pub fn ai_transport_session_action_accepted(
    action: &str,
    session_id: oxideterm_workspace::TerminalSessionId,
) -> Value {
    serde_json::json!({"accepted": true, "action": action, "sessionId": session_id.0.to_string()})
}

pub struct AiSettingsWrite<'a> {
    pub settings: oxideterm_settings::PersistedSettings,
    pub section: &'a str,
    pub result: AiActionResultLite,
}

pub fn plan_ai_settings_write<'a>(
    snapshot: &AiOrchestratorRuntimeSnapshot,
    current: &oxideterm_settings::PersistedSettings,
    args: &'a Value,
) -> Result<AiSettingsWrite<'a>, AiActionResultLite> {
    if !matches!(
        ai_stable_resource_operation("write_resource", args),
        Ok(AiStableResourceOperation::Settings)
    ) {
        return Err(snapshot.fail(
            "Settings resource is unavailable.",
            "resource_not_found",
            "Rediscover the application settings resource before updating it.",
            "write",
        ));
    }
    let Some(target) = snapshot
        .targets
        .iter()
        .find(|target| target.kind == "settings")
        .cloned()
    else {
        return Err(snapshot.fail(
            "Settings resource is unavailable.",
            "resource_not_found",
            "Rediscover the application settings resource before updating it.",
            "write",
        ));
    };
    let Some(section) = args.get("section").and_then(serde_json::Value::as_str) else {
        return Err(snapshot.fail(
            "Settings section and key are required.",
            "missing_settings_key",
            "write_resource(settings) requires section and key.",
            "write",
        ));
    };
    let Some(key) = args.get("key").and_then(serde_json::Value::as_str) else {
        return Err(snapshot.fail(
            "Settings section and key are required.",
            "missing_settings_key",
            "write_resource(settings) requires section and key.",
            "write",
        ));
    };
    let value = args
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    if args
        .get("dry_run")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
    {
        return Err(snapshot
            .ok(
                format!("Dry-run settings write {section}.{key}."),
                "Dry-run only; settings were not changed.",
                serde_json::json!({ "section": section, "key": key, "value": value }),
                "write",
            )
            .with_target(target)
            .with_verified(false));
    }
    let settings =
        settings_with_json_patch(current, section, key, value.clone()).map_err(|error| {
            snapshot
                .fail(
                    "Settings section cannot be updated.",
                    "unsupported_settings_section",
                    error,
                    "write",
                )
                .with_target(target.clone())
        })?;
    let result = snapshot.ok(
            format!("Updated settings {section}.{key}."), format!("{section}.{key} updated."),
            serde_json::json!({ "section": section, "key": key, "value": value, "visibleSurface": "settings" }),
            "write",
        ).with_target(target);
    Ok(AiSettingsWrite {
        settings,
        section,
        result,
    })
}

fn settings_with_json_patch(
    settings: &oxideterm_settings::PersistedSettings,
    section: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<oxideterm_settings::PersistedSettings, String> {
    let mut root = serde_json::to_value(settings).map_err(|error| error.to_string())?;
    let Some(section_value) = root.get_mut(section) else {
        return Err(format!("No settings section named {section}."));
    };
    let Some(section_object) = section_value.as_object_mut() else {
        return Err(format!("Settings section {section} cannot be updated."));
    };
    section_object.insert(key.to_string(), value);
    serde_json::from_value(root).map_err(|error| error.to_string())
}

#[derive(Clone, Copy)]
pub enum AiTransportLifecycle {
    Running,
    Exited(Option<i32>),
    Closed,
}

impl AiTransportLifecycle {
    fn json(self) -> Value {
        match self {
            Self::Running => {
                serde_json::json!({ "state": "running" })
            }
            Self::Exited(exit_code) => {
                serde_json::json!({ "state": "exited", "exitCode": exit_code })
            }
            Self::Closed => {
                serde_json::json!({ "state": "closed" })
            }
        }
    }
    pub fn telnet(self) -> Value {
        serde_json::json!({ "transport": "telnet", "lifecycle": self.json() })
    }
}

pub struct AiSerialSessionCapture {
    pub port: String,
    pub available: Option<bool>,
    pub baud_rate: u32,
    pub data_bits: u8,
    pub stop_bits: u8,
    pub parity: String,
    pub flow_control: String,
    pub dtr: bool,
    pub rts: bool,
    pub local_echo: bool,
    pub line_ending: String,
    pub output_line_ending: String,
    pub display_mode: String,
    pub send_mode: String,
    pub can_reconnect: bool,
}

impl AiSerialSessionCapture {
    pub fn json(&self, lifecycle: AiTransportLifecycle) -> Value {
        serde_json::json!({
            "transport": "serial",
            "lifecycle": lifecycle.json(),
            "port": {
                "path": self.port,
                "available": self.available,
                "baudRate": self.baud_rate,
                "dataBits": self.data_bits,
                "stopBits": self.stop_bits,
                "parity": self.parity,
                "flowControl": self.flow_control,
            },
            "controlLines": {
                "dtr": self.dtr,
                "rts": self.rts,
            },
            "runtime": {
                "localEcho": self.local_echo,
                "lineEnding": self.line_ending,
                "outputLineEnding": self.output_line_ending,
                "displayMode": self.display_mode,
                "sendMode": self.send_mode,
            },
            "canReconnect": self.can_reconnect,
        })
    }
}

impl AiOrchestratorRuntimeSnapshot {
    pub fn plan_saved_connection(
        &self,
        args: &Value,
    ) -> Result<crate::StableResourceRef, AiActionResultLite> {
        match ai_stable_resource_operation("connect_target", args) {
            Ok(AiStableResourceOperation::SavedConnection(resource)) => Ok(resource),
            _ => Err(self.fail(
                "Saved connection is unavailable.",
                "resource_not_found",
                "Rediscover the saved connection before connecting.",
                "write",
            )),
        }
    }

    fn saved_connection_target(
        &self,
        resource: &crate::StableResourceRef,
    ) -> Option<AiOrchestratorTarget> {
        self.targets
            .iter()
            .find(|target| {
                target.kind == "saved-connection"
                    && target
                        .refs
                        .get("connectionId")
                        .is_some_and(|id| id == resource.id())
            })
            .cloned()
    }

    pub fn saved_connection_removed(&self) -> AiActionResultLite {
        self.fail(
            "Saved connection was removed.",
            "resource_removed",
            "The saved connection no longer exists. Rediscover available connections.",
            "write",
        )
    }

    pub fn saved_connection_credentials_required(
        &self,
        resource: &crate::StableResourceRef,
    ) -> AiActionResultLite {
        self.fail(
            "Saved connection cannot be opened.",
            "credential_interaction_required",
            "The saved connection needs valid SSH configuration or credentials.",
            "write",
        )
        .with_optional_target(self.saved_connection_target(resource))
    }

    pub fn saved_connection_opened(
        &self,
        resource: crate::StableResourceRef,
        reused: bool,
    ) -> AiActionResultLite {
        let (summary, output) = if reused {
            (
                "Target is already live.",
                "Focused the existing SSH terminal.",
            )
        } else {
            (
                "Connection requested.",
                "The saved connection flow has started.",
            )
        };
        self.ok(
            summary,
            output,
            serde_json::json!({ "resourceRef": resource }),
            "write",
        )
        .with_optional_target(self.saved_connection_target(&resource))
    }
}

pub fn ai_connection_title(name: &str, username: &str, host: &str) -> String {
    if name.trim().is_empty() {
        format!("{username}@{host}")
    } else {
        name.to_owned()
    }
}

pub fn ai_host_tools_query(arguments: &Value) -> (&str, bool) {
    let resource = arguments
        .get("resource")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("overview");
    let refresh_requested = arguments
        .get("refresh")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    (resource, refresh_requested)
}

pub fn ai_host_tools_inspection(
    resource: &str,
    refresh_requested: bool,
    refresh_accepted: bool,
    snapshot: Value,
) -> Value {
    serde_json::json!({
        "node": {
            "state": "connected",
        },
        "resource": resource,
        "refreshRequested": refresh_requested,
        "refreshAccepted": refresh_accepted,
        "snapshot": snapshot,
    })
}

pub struct AiHostToolControl<'a> {
    pub resource: &'a str,
    pub action: &'a str,
    pub entity_id: &'a str,
    pub value: Option<&'a str>,
}

pub fn ai_host_tool_control(arguments: &Value) -> Result<AiHostToolControl<'_>, String> {
    let resource = arguments
        .get("resource")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A Host Tools resource is required.".to_string())?;
    let action = arguments
        .get("action")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A Host Tools action is required.".to_string())?;
    let entity_id = arguments
        .get("entity_id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A Host Tools entity identifier is required.".to_string())?;
    let value = arguments.get("value").and_then(serde_json::Value::as_str);
    Ok(AiHostToolControl {
        resource,
        action,
        entity_id,
        value,
    })
}

impl AiHostToolControl<'_> {
    pub fn accepted(&self) -> Value {
        serde_json::json!({
            "accepted": true, "resource": self.resource,
            "action": self.action, "entityId": self.entity_id,
        })
    }
}

pub enum AiCloudSyncAction {
    Open,
    Check,
    UploadPreview,
    PullPreview,
}

pub fn ai_cloud_sync_action(arguments: &Value, busy: bool) -> Result<AiCloudSyncAction, String> {
    let action = arguments
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| "A Cloud Sync action is required.".to_string())?;
    if action != "open" && busy {
        return Err("Another Cloud Sync operation is already running.".to_string());
    }
    match action {
        "open" => Ok(AiCloudSyncAction::Open),
        "check" => Ok(AiCloudSyncAction::Check),
        "upload_preview" => Ok(AiCloudSyncAction::UploadPreview),
        "pull_preview" => Ok(AiCloudSyncAction::PullPreview),
        _ => Err("Unsupported Cloud Sync action.".to_string()),
    }
}

impl AiCloudSyncAction {
    pub fn accepted(self) -> Value {
        ai_action_accepted(match self {
            Self::Open => "open",
            Self::Check => "check",
            Self::UploadPreview => "upload_preview",
            Self::PullPreview => "pull_preview",
        })
    }
}

pub enum AiProfileTransport {
    Serial,
    Telnet,
    RemoteDesktop,
}

pub struct AiOpenTransportProfile<'a> {
    pub kind: AiProfileTransport,
    pub id: &'a str,
    transport: &'a str,
}

pub fn ai_open_transport_profile<'a>(
    arguments: &'a Value,
    serial: &[oxideterm_connections::SerialProfile],
    telnet: &[oxideterm_connections::TelnetProfile],
    remote_desktop: &[oxideterm_connections::RemoteDesktopProfile],
) -> Result<AiOpenTransportProfile<'a>, String> {
    let transport = arguments
        .get("transport")
        .and_then(Value::as_str)
        .ok_or_else(|| "A transport is required.".to_string())?;
    let id = arguments
        .get("profile_id")
        .and_then(Value::as_str)
        .ok_or_else(|| "A profile identifier is required.".to_string())?;
    let kind = match transport {
        "serial" if serial.iter().any(|profile| profile.id == id) => AiProfileTransport::Serial,
        "telnet" if telnet.iter().any(|profile| profile.id == id) => AiProfileTransport::Telnet,
        "rdp" | "vnc" => {
            let profile = remote_desktop
                .iter()
                .find(|profile| profile.id == id)
                .ok_or_else(|| "The remote desktop profile no longer exists.".to_string())?;
            if profile.protocol.provider_id() != transport {
                return Err(
                    "The profile protocol does not match the requested transport.".to_string(),
                );
            }
            AiProfileTransport::RemoteDesktop
        }
        _ => return Err("The selected transport profile no longer exists.".to_string()),
    };
    Ok(AiOpenTransportProfile {
        kind,
        id,
        transport,
    })
}

impl AiOpenTransportProfile<'_> {
    pub fn accepted(&self) -> Value {
        serde_json::json!({"accepted": true, "transport": self.transport, "profileId": self.id})
    }
}

pub fn ai_remote_desktop_tab(arguments: &Value) -> Result<oxideterm_workspace::TabId, String> {
    arguments
        .get("tab_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| value.parse::<u64>().ok())
        .map(oxideterm_workspace::TabId)
        .ok_or_else(|| "A valid remote desktop tab identifier is required.".to_string())
}

pub fn ai_remote_desktop_disconnect(
    arguments: &Value,
    tab_id: oxideterm_workspace::TabId,
    can_disconnect: impl FnOnce() -> bool,
) -> Result<Value, String> {
    let action = arguments
        .get("action")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "A remote desktop session action is required.".to_string())?;
    match action {
        "disconnect" => {
            if !can_disconnect() {
                return Err("The remote desktop session is already disconnected.".to_string());
            }
        }
        "reconnect" => {
            return Err(
                "Remote desktop reconnect creates a new tab from Active Sessions.".to_string(),
            );
        }
        _ => return Err("Unsupported remote desktop session action.".to_string()),
    }
    Ok(serde_json::json!({
        "accepted": true,
        "action": action,
        "tabId": tab_id.0.to_string(),
    }))
}

pub fn ai_connection_credential_inventory(
    store: &oxideterm_connections::ConnectionStore,
    arguments: &Value,
) -> Result<Value, String> {
    let scopes = ai_credential_scopes(
        arguments,
        store
            .connections()
            .iter()
            .map(|connection| connection.id.as_str()),
    );
    let mut privilege_credentials = Vec::new();
    for scope in &scopes {
        privilege_credentials.extend(
            store
                .list_privilege_credentials(scope)
                .map_err(|error| error.to_string())?,
        );
    }
    Ok(ai_list_credential_metadata(
        store.managed_ssh_keys(),
        privilege_credentials,
        store.remote_desktop_profiles(),
    ))
}

impl AiCredentialKind<'_> {
    pub fn delete_from(
        &self,
        store: &mut oxideterm_connections::ConnectionStore,
        id: &str,
    ) -> Result<bool, String> {
        match *self {
            AiCredentialKind::ManagedSshKey { force } => store
                .delete_managed_ssh_key(id, force)
                .map(|result| result.deleted),
            AiCredentialKind::Privilege { connection_id } => {
                store.delete_privilege_credential(connection_id, id)
            }
            AiCredentialKind::RemoteDesktop => store.delete_remote_desktop_credential(id),
        }
        .map_err(|error| error.to_string())
    }
}
