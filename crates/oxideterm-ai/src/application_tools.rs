// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde_json::{Map, Value, json};

use crate::{AiToolDefinition, OrchestratorArgumentError};

const MAX_IDENTIFIER_CHARS: usize = 256;
const MAX_TASK_TITLE_CHARS: usize = 160;
const MAX_TASK_ARGUMENT_NODES: usize = 4_096;
const MAX_TASK_ARGUMENT_STRING_CHARS: usize = 65_536;
const MAX_URL_CHARS: usize = 4_096;

pub(crate) fn extended_application_tool_definitions() -> Vec<AiToolDefinition> {
    vec![
        tool(
            "create_background_task",
            "Create a cancellable one-shot task or safe recurring monitor. Recurring tasks can call only monitor-safe read tools and must use stable resource references instead of live handles.",
            json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "maxLength": MAX_TASK_TITLE_CHARS },
                    "tool_name": {
                        "type": "string",
                        "enum": ["list_targets", "get_state", "read_resource", "inspect_host_tools", "list_forwards", "list_plugins"]
                    },
                    "arguments": { "type": "object", "description": "Arguments for the nested monitor-safe tool." },
                    "mode": { "type": "string", "enum": ["one_shot", "interval", "condition"] },
                    "interval_seconds": { "type": "integer", "minimum": 5, "maximum": 86400 },
                    "max_runs": { "type": "integer", "minimum": 1, "maximum": 10000 },
                    "condition": { "type": "string", "enum": ["result_changed", "result_contains", "result_field_equals", "execution_fails", "execution_recovers"] },
                    "condition_text": { "type": "string", "maxLength": 4096 },
                    "condition_pointer": { "type": "string", "maxLength": 1024 },
                    "condition_value": {}
                },
                "required": ["tool_name", "arguments", "mode"],
                "additionalProperties": false
            }),
        ),
        tool(
            "list_background_tasks",
            "List background tasks owned by the current OxideSens conversation.",
            empty_object_schema(),
        ),
        tool(
            "get_background_task",
            "Inspect one background task owned by the current OxideSens conversation.",
            id_schema("task_id"),
        ),
        tool(
            "cancel_background_task",
            "Cancel one running or waiting background task owned by the current OxideSens conversation.",
            id_schema("task_id"),
        ),
        tool(
            "inspect_host_tools",
            "Read an existing Host Tools snapshot or request a fresh snapshot through the selected live host. This reuses Host Tools and never opens another SSH connection.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64 },
                    "resource_ref": {
                        "type": "object",
                        "properties": {
                            "kind": { "const": "saved_connection" },
                            "id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                            "label": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS }
                        },
                        "required": ["kind", "id"],
                        "additionalProperties": false
                    },
                    "resource": { "type": "string", "enum": ["overview", "processes", "docker", "services", "tmux", "ports", "filesystems", "packages", "schedules", "logs"] },
                    "refresh": { "type": "boolean" }
                },
                "required": ["resource"],
                "oneOf": [
                    { "required": ["handle_id"] },
                    { "required": ["resource_ref"] }
                ],
                "additionalProperties": false
            }),
        ),
        tool(
            "control_host_tool",
            "Run a supported Host Tools mutation using its existing availability checks and runtime owner.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64 },
                    "resource": { "type": "string", "enum": ["process", "docker", "service", "tmux", "schedule"] },
                    "action": { "type": "string", "enum": ["signal", "start", "stop", "restart", "kill_session", "kill_window", "kill_pane", "rename_session", "rename_window", "send_pane_command", "enable", "disable", "run"] },
                    "entity_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "value": { "type": "string", "maxLength": 65536 }
                },
                "required": ["handle_id", "resource", "action", "entity_id"],
                "additionalProperties": false
            }),
        ),
        tool(
            "list_forwards",
            "List forwarding rules and runtime status for a current SSH node or stable saved connection.",
            authority_schema(),
        ),
        tool(
            "manage_forward",
            "Create, update, stop, restart, or delete a forwarding rule through the node-owned forwarding runtime.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64 },
                    "action": { "type": "string", "enum": ["create", "update", "stop", "restart", "delete", "scan_ports"] },
                    "forward_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "rule": { "type": "object" }
                },
                "required": ["handle_id", "action"],
                "additionalProperties": false
            }),
        ),
        tool(
            "list_plugins",
            "List installed plugins, runtime state, capabilities, and granted permissions without exposing plugin secrets.",
            empty_object_schema(),
        ),
        tool(
            "manage_plugin",
            "Install, enable, disable, configure, uninstall, invoke, or open the manager for a native plugin. Password-type settings cannot be configured through AI.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["install", "enable", "disable", "configure", "uninstall", "invoke", "open_manager"] },
                    "plugin_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "command_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "arguments": { "type": "object" },
                    "remove_storage": { "type": "boolean" },
                    "package_url": { "type": "string", "maxLength": MAX_URL_CHARS },
                    "checksum": { "type": "string", "maxLength": 256 },
                    "overwrite": { "type": "boolean" },
                    "setting_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "value": {}
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        ),
        tool(
            "list_transport_profiles",
            "List saved serial, Telnet, RDP, and VNC profiles using metadata only. Credentials are never returned.",
            empty_object_schema(),
        ),
        tool(
            "open_transport_profile",
            "Open a saved serial, Telnet, RDP, or VNC profile through its existing workspace session owner.",
            json!({
                "type": "object",
                "properties": {
                    "transport": { "type": "string", "enum": ["serial", "telnet", "rdp", "vnc"] },
                    "profile_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS }
                },
                "required": ["transport", "profile_id"],
                "additionalProperties": false
            }),
        ),
        tool(
            "get_transport_session_state",
            "Read the live transport and lifecycle state for an existing serial or Telnet terminal. Serial sessions also report control lines and runtime modes.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64 }
                },
                "required": ["handle_id"],
                "additionalProperties": false
            }),
        ),
        tool(
            "manage_serial_session",
            "Control an existing serial terminal without opening another port. Supports reconnect, port refresh, break, DTR/RTS, local echo, line endings, and display/send modes.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64 },
                    "action": {
                        "type": "string",
                        "enum": ["refresh_port", "reconnect", "send_break", "set_dtr", "set_rts", "set_local_echo", "set_line_ending", "set_display_mode", "set_send_mode"]
                    },
                    "enabled": { "type": "boolean" },
                    "value": {
                        "type": "string",
                        "enum": ["none", "lf", "crlf", "cr", "text", "hex", "mixed"]
                    }
                },
                "required": ["handle_id", "action"],
                "additionalProperties": false
            }),
        ),
        tool(
            "manage_telnet_session",
            "Send a Telnet protocol control command or disconnect an existing Telnet session. Protocol controls are transmitted as IAC commands rather than escaped terminal text.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64 },
                    "action": {
                        "type": "string",
                        "enum": ["noop", "break", "interrupt_process", "abort_output", "are_you_there", "erase_character", "erase_line", "go_ahead", "disconnect"]
                    }
                },
                "required": ["handle_id", "action"],
                "additionalProperties": false
            }),
        ),
        tool(
            "list_remote_desktop_sessions",
            "List live RDP and VNC session state, capabilities, endpoint metadata, and owning tab identifiers.",
            empty_object_schema(),
        ),
        tool(
            "manage_remote_desktop_session",
            "Disconnect or reconnect one existing RDP or VNC session without affecting unrelated sessions.",
            json!({
                "type": "object",
                "properties": {
                    "tab_id": { "type": "string", "maxLength": 128 },
                    "action": { "type": "string", "enum": ["disconnect", "reconnect"] }
                },
                "required": ["tab_id", "action"],
                "additionalProperties": false
            }),
        ),
        tool(
            "get_cloud_sync_state",
            "Read non-secret Cloud Sync configuration, sync scope, operation state, conflict state, dirty state, and recent history.",
            empty_object_schema(),
        ),
        tool(
            "configure_cloud_sync",
            "Update non-secret Cloud Sync backend, scheduling, conflict, OAuth client-ID, and sync-scope settings. Existing passwords, tokens, and protected credentials are left unchanged.",
            json!({
                "type": "object",
                "properties": {
                    "backend_type": { "type": "string", "enum": ["webdav", "http-json", "dropbox", "one-drive", "google-drive", "github-gist", "s3", "git"] },
                    "auth_mode": { "type": "string", "enum": ["bearer", "basic", "none"] },
                    "endpoint": { "type": "string", "maxLength": MAX_URL_CHARS },
                    "namespace": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "s3_bucket": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "s3_region": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "git_repository": { "type": "string", "maxLength": MAX_URL_CHARS },
                    "git_branch": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "github_oauth_client_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "microsoft_oauth_client_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "google_oauth_client_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "auto_upload_enabled": { "type": "boolean" },
                    "auto_upload_interval_mins": { "type": "number", "exclusiveMinimum": 0 },
                    "default_conflict_strategy": { "type": "string", "enum": ["merge", "replace", "skip", "rename"] },
                    "scope": {
                        "type": "object",
                        "properties": {
                            "sync_connections": { "type": "boolean" },
                            "sync_forwards": { "type": "boolean" },
                            "sync_quick_commands": { "type": "boolean" },
                            "sync_serial_profiles": { "type": "boolean" },
                            "sync_mosh_profiles": { "type": "boolean" },
                            "sync_remote_desktop_profiles": { "type": "boolean" },
                            "sync_sensitive_credentials": { "type": "boolean" },
                            "sync_app_settings": { "type": "boolean" },
                            "app_settings_sections": { "type": "array", "items": { "type": "string", "maxLength": 64 }, "maxItems": 32 },
                            "include_local_terminal_env_vars": { "type": "boolean" },
                            "sync_plugin_settings": { "type": "boolean" },
                            "plugin_ids": { "type": "array", "items": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS }, "maxItems": 256 }
                        },
                        "minProperties": 1,
                        "additionalProperties": false
                    }
                },
                "minProperties": 1,
                "additionalProperties": false
            }),
        ),
        tool(
            "manage_cloud_sync",
            "Open Cloud Sync or start a check, upload preview, or pull preview. Applying or overwriting remote data remains a user-confirmed UI action.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["open", "check", "upload_preview", "pull_preview"] }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        ),
        tool(
            "list_credentials",
            "List managed SSH key and privilege-credential metadata. Secret values, private keys, passwords, and passphrases are never returned.",
            json!({
                "type": "object",
                "properties": {
                    "connection_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "manage_credential",
            "Open credential management or delete one managed SSH key, privilege credential, or saved remote-desktop credential. AI cannot read or create raw secret values.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["open_manager", "delete"] },
                    "kind": { "type": "string", "enum": ["managed_ssh_key", "privilege", "remote_desktop"] },
                    "id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "connection_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "force": { "type": "boolean" }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        ),
        tool(
            "list_memory_entries",
            "List non-expired memory entries visible to a requested user, workspace, project, or host scope, including provenance and usage metadata.",
            json!({
                "type": "object",
                "properties": {
                    "scope_kind": { "type": "string", "enum": ["user", "workspace", "project", "host"] },
                    "scope_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS, "description": "Optional explicit scope identity. When omitted, OxideTerm resolves the current user, workspace, project, or host." },
                    "memory_kind": { "type": "string", "enum": ["long_term", "temporary"] },
                    "include_expired": { "type": "boolean" }
                },
                "additionalProperties": false
            }),
        ),
        tool(
            "manage_memory_entry",
            "Create, update, delete, or mark one scoped memory entry as used. Updates require the current revision so concurrent edits cannot silently overwrite each other.",
            json!({
                "type": "object",
                "properties": {
                    "action": { "type": "string", "enum": ["create", "update", "delete", "touch"] },
                    "id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "content": { "type": "string", "maxLength": 16000 },
                    "scope_kind": { "type": "string", "enum": ["user", "workspace", "project", "host"] },
                    "scope_id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS, "description": "Optional explicit scope identity. When omitted, OxideTerm resolves the current user, workspace, project, or host." },
                    "memory_kind": { "type": "string", "enum": ["long_term", "temporary"] },
                    "expires_at_ms": { "type": "integer", "minimum": 1 },
                    "expected_revision": { "type": "integer", "minimum": 1 }
                },
                "required": ["action"],
                "additionalProperties": false
            }),
        ),
    ]
}

pub(crate) fn validate_extended_application_tool_arguments(
    tool_name: &str,
    object: &Map<String, Value>,
) -> Result<bool, OrchestratorArgumentError> {
    match tool_name {
        "create_background_task" => validate_create_background_task(object)?,
        "list_background_tasks"
        | "list_plugins"
        | "list_transport_profiles"
        | "list_remote_desktop_sessions"
        | "get_cloud_sync_state" => require_fields(object, &[], &[])?,
        "get_background_task" | "cancel_background_task" => {
            require_fields(object, &["task_id"], &["task_id"])?;
            required_string(object, "task_id", MAX_IDENTIFIER_CHARS)?;
        }
        "inspect_host_tools" => validate_inspect_host_tools(object)?,
        "control_host_tool" => validate_control_host_tool(object)?,
        "list_forwards" => validate_authority(object)?,
        "manage_forward" => validate_manage_forward(object)?,
        "manage_plugin" => validate_manage_plugin(object)?,
        "open_transport_profile" => {
            require_fields(
                object,
                &["transport", "profile_id"],
                &["transport", "profile_id"],
            )?;
            required_enum(object, "transport", &["serial", "telnet", "rdp", "vnc"])?;
            required_string(object, "profile_id", MAX_IDENTIFIER_CHARS)?;
        }
        "get_transport_session_state" => {
            require_fields(object, &["handle_id"], &["handle_id"])?;
            required_string(object, "handle_id", 64)?;
        }
        "manage_serial_session" => validate_manage_serial_session(object)?,
        "manage_telnet_session" => {
            require_fields(object, &["handle_id", "action"], &["handle_id", "action"])?;
            required_string(object, "handle_id", 64)?;
            required_enum(
                object,
                "action",
                &[
                    "noop",
                    "break",
                    "interrupt_process",
                    "abort_output",
                    "are_you_there",
                    "erase_character",
                    "erase_line",
                    "go_ahead",
                    "disconnect",
                ],
            )?;
        }
        "manage_remote_desktop_session" => {
            require_fields(object, &["tab_id", "action"], &["tab_id", "action"])?;
            required_string(object, "tab_id", 128)?;
            required_enum(object, "action", &["disconnect", "reconnect"])?;
        }
        "manage_cloud_sync" => {
            require_fields(object, &["action"], &["action"])?;
            required_enum(
                object,
                "action",
                &["open", "check", "upload_preview", "pull_preview"],
            )?;
        }
        "configure_cloud_sync" => validate_configure_cloud_sync(object)?,
        "list_credentials" => {
            require_fields(object, &["connection_id"], &[])?;
            optional_string(object, "connection_id", MAX_IDENTIFIER_CHARS)?;
        }
        "manage_credential" => validate_manage_credential(object)?,
        "list_memory_entries" => {
            require_fields(
                object,
                &["scope_kind", "scope_id", "memory_kind", "include_expired"],
                &[],
            )?;
            optional_enum(
                object,
                "scope_kind",
                &["user", "workspace", "project", "host"],
            )?;
            optional_string(object, "scope_id", MAX_IDENTIFIER_CHARS)?;
            optional_enum(object, "memory_kind", &["long_term", "temporary"])?;
            optional_bool(object, "include_expired")?;
        }
        "manage_memory_entry" => validate_manage_memory_entry(object)?,
        _ => return Ok(false),
    }
    Ok(true)
}

fn validate_configure_cloud_sync(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &[
            "backend_type",
            "auth_mode",
            "endpoint",
            "namespace",
            "s3_bucket",
            "s3_region",
            "git_repository",
            "git_branch",
            "github_oauth_client_id",
            "microsoft_oauth_client_id",
            "google_oauth_client_id",
            "auto_upload_enabled",
            "auto_upload_interval_mins",
            "default_conflict_strategy",
            "scope",
        ],
        &[],
    )?;
    if object.is_empty() {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    optional_enum(
        object,
        "backend_type",
        &[
            "webdav",
            "http-json",
            "dropbox",
            "one-drive",
            "google-drive",
            "github-gist",
            "s3",
            "git",
        ],
    )?;
    optional_enum(object, "auth_mode", &["bearer", "basic", "none"])?;
    optional_string_allow_empty(object, "endpoint", MAX_URL_CHARS)?;
    for field in [
        "namespace",
        "s3_bucket",
        "s3_region",
        "git_branch",
        "github_oauth_client_id",
        "microsoft_oauth_client_id",
        "google_oauth_client_id",
    ] {
        optional_string_allow_empty(object, field, MAX_IDENTIFIER_CHARS)?;
    }
    optional_string_allow_empty(object, "git_repository", MAX_URL_CHARS)?;
    optional_bool(object, "auto_upload_enabled")?;
    optional_positive_number(object, "auto_upload_interval_mins")?;
    optional_enum(
        object,
        "default_conflict_strategy",
        &["merge", "replace", "skip", "rename"],
    )?;

    if let Some(scope) = object.get("scope") {
        let scope = scope
            .as_object()
            .ok_or(OrchestratorArgumentError::InvalidArguments)?;
        let boolean_fields = [
            "sync_connections",
            "sync_forwards",
            "sync_quick_commands",
            "sync_serial_profiles",
            "sync_mosh_profiles",
            "sync_remote_desktop_profiles",
            "sync_sensitive_credentials",
            "sync_app_settings",
            "include_local_terminal_env_vars",
            "sync_plugin_settings",
        ];
        let mut allowed_fields = boolean_fields.to_vec();
        allowed_fields.extend(["app_settings_sections", "plugin_ids"]);
        require_fields(scope, &allowed_fields, &[])?;
        if scope.is_empty() {
            return Err(OrchestratorArgumentError::InvalidArguments);
        }
        for field in boolean_fields {
            optional_bool(scope, field)?;
        }
        optional_string_array(scope, "app_settings_sections", 32, 64)?;
        optional_string_array(scope, "plugin_ids", 256, MAX_IDENTIFIER_CHARS)?;
    }
    Ok(())
}

fn validate_manage_serial_session(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &["handle_id", "action", "enabled", "value"],
        &["handle_id", "action"],
    )?;
    required_string(object, "handle_id", 64)?;
    let action = required_enum(
        object,
        "action",
        &[
            "refresh_port",
            "reconnect",
            "send_break",
            "set_dtr",
            "set_rts",
            "set_local_echo",
            "set_line_ending",
            "set_display_mode",
            "set_send_mode",
        ],
    )?;
    match action {
        "set_dtr" | "set_rts" | "set_local_echo" => {
            required_bool(object, "enabled")?;
            if object.contains_key("value") {
                return Err(OrchestratorArgumentError::InvalidArguments);
            }
        }
        "set_line_ending" => {
            required_enum(object, "value", &["none", "lf", "crlf", "cr"])?;
            if object.contains_key("enabled") {
                return Err(OrchestratorArgumentError::InvalidArguments);
            }
        }
        "set_display_mode" => {
            required_enum(object, "value", &["text", "hex", "mixed"])?;
            if object.contains_key("enabled") {
                return Err(OrchestratorArgumentError::InvalidArguments);
            }
        }
        "set_send_mode" => {
            required_enum(object, "value", &["text", "hex"])?;
            if object.contains_key("enabled") {
                return Err(OrchestratorArgumentError::InvalidArguments);
            }
        }
        _ => {
            if object.contains_key("enabled") || object.contains_key("value") {
                return Err(OrchestratorArgumentError::InvalidArguments);
            }
        }
    }
    Ok(())
}

fn validate_manage_memory_entry(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &[
            "action",
            "id",
            "content",
            "scope_kind",
            "scope_id",
            "memory_kind",
            "expires_at_ms",
            "expected_revision",
        ],
        &["action"],
    )?;
    let action = required_enum(object, "action", &["create", "update", "delete", "touch"])?;
    match action {
        "create" => {
            required_string(object, "content", 16_000)?;
            required_enum(
                object,
                "scope_kind",
                &["user", "workspace", "project", "host"],
            )?;
            optional_string(object, "scope_id", MAX_IDENTIFIER_CHARS)?;
            optional_enum(object, "memory_kind", &["long_term", "temporary"])?;
            optional_integer(object, "expires_at_ms", 1, i64::MAX as u64)?;
            if object.contains_key("expected_revision") {
                return Err(OrchestratorArgumentError::InvalidArguments);
            }
        }
        "update" => {
            required_string(object, "id", MAX_IDENTIFIER_CHARS)?;
            required_integer(object, "expected_revision", 1, i64::MAX as u64)?;
            optional_string(object, "content", 16_000)?;
            optional_enum(
                object,
                "scope_kind",
                &["user", "workspace", "project", "host"],
            )?;
            optional_string(object, "scope_id", MAX_IDENTIFIER_CHARS)?;
            if object.contains_key("scope_id") && !object.contains_key("scope_kind") {
                return Err(OrchestratorArgumentError::InvalidArguments);
            }
            optional_enum(object, "memory_kind", &["long_term", "temporary"])?;
            optional_integer(object, "expires_at_ms", 1, i64::MAX as u64)?;
        }
        "delete" | "touch" => {
            required_string(object, "id", MAX_IDENTIFIER_CHARS)?;
            required_integer(object, "expected_revision", 1, i64::MAX as u64)?;
        }
        _ => return Err(OrchestratorArgumentError::InvalidArguments),
    }
    Ok(())
}

fn validate_create_background_task(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &[
            "title",
            "tool_name",
            "arguments",
            "mode",
            "interval_seconds",
            "max_runs",
            "condition",
            "condition_text",
            "condition_pointer",
            "condition_value",
        ],
        &["tool_name", "arguments", "mode"],
    )?;
    optional_string(object, "title", MAX_TASK_TITLE_CHARS)?;
    required_enum(
        object,
        "tool_name",
        &[
            "list_targets",
            "get_state",
            "read_resource",
            "inspect_host_tools",
            "list_forwards",
            "list_plugins",
        ],
    )?;
    let arguments = object
        .get("arguments")
        .and_then(Value::as_object)
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    let mut nodes = MAX_TASK_ARGUMENT_NODES;
    if !bounded_json(&Value::Object(arguments.clone()), &mut nodes)
        || contains_live_handle(arguments)
    {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    let mode = required_enum(object, "mode", &["one_shot", "interval", "condition"])?;
    if mode == "one_shot" {
        if object.keys().any(|key| {
            matches!(
                key.as_str(),
                "interval_seconds"
                    | "max_runs"
                    | "condition"
                    | "condition_text"
                    | "condition_pointer"
                    | "condition_value"
            )
        }) {
            return Err(OrchestratorArgumentError::InvalidArguments);
        }
        return Ok(());
    }
    required_integer(object, "interval_seconds", 5, 86_400)?;
    required_integer(object, "max_runs", 1, 10_000)?;
    if mode == "interval" {
        if object.keys().any(|key| key.starts_with("condition")) {
            return Err(OrchestratorArgumentError::InvalidArguments);
        }
        return Ok(());
    }
    let condition = required_enum(
        object,
        "condition",
        &[
            "result_changed",
            "result_contains",
            "result_field_equals",
            "execution_fails",
            "execution_recovers",
        ],
    )?;
    match condition {
        "result_contains" => required_string(object, "condition_text", 4_096).map(|_| ()),
        "result_field_equals" => {
            required_string(object, "condition_pointer", 1_024)?;
            object
                .contains_key("condition_value")
                .then_some(())
                .ok_or(OrchestratorArgumentError::InvalidArguments)
        }
        _ => Ok(()),
    }
}

fn validate_inspect_host_tools(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &["handle_id", "resource_ref", "resource", "refresh"],
        &["resource"],
    )?;
    validate_exact_authority(object)?;
    required_enum(
        object,
        "resource",
        &[
            "overview",
            "processes",
            "docker",
            "services",
            "tmux",
            "ports",
            "filesystems",
            "packages",
            "schedules",
            "logs",
        ],
    )?;
    optional_bool(object, "refresh")
}

fn validate_control_host_tool(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &["handle_id", "resource", "action", "entity_id", "value"],
        &["handle_id", "resource", "action", "entity_id"],
    )?;
    required_string(object, "handle_id", 64)?;
    required_enum(
        object,
        "resource",
        &["process", "docker", "service", "tmux", "schedule"],
    )?;
    required_enum(
        object,
        "action",
        &[
            "signal",
            "start",
            "stop",
            "restart",
            "kill_session",
            "kill_window",
            "kill_pane",
            "rename_session",
            "rename_window",
            "send_pane_command",
            "enable",
            "disable",
            "run",
        ],
    )?;
    required_string(object, "entity_id", MAX_IDENTIFIER_CHARS)?;
    optional_string(object, "value", 65_536)
}

fn validate_authority(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_fields(object, &["handle_id", "resource_ref"], &[])?;
    validate_exact_authority(object)
}

fn validate_manage_forward(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &["handle_id", "action", "forward_id", "rule"],
        &["handle_id", "action"],
    )?;
    required_string(object, "handle_id", 64)?;
    let action = required_enum(
        object,
        "action",
        &[
            "create",
            "update",
            "stop",
            "restart",
            "delete",
            "scan_ports",
        ],
    )?;
    match action {
        "create" => object
            .get("rule")
            .and_then(Value::as_object)
            .map(|_| ())
            .ok_or(OrchestratorArgumentError::InvalidArguments),
        "update" => {
            required_string(object, "forward_id", MAX_IDENTIFIER_CHARS)?;
            object
                .get("rule")
                .and_then(Value::as_object)
                .map(|_| ())
                .ok_or(OrchestratorArgumentError::InvalidArguments)
        }
        "stop" | "restart" | "delete" => {
            required_string(object, "forward_id", MAX_IDENTIFIER_CHARS).map(|_| ())
        }
        "scan_ports" => Ok(()),
        _ => Err(OrchestratorArgumentError::InvalidArguments),
    }
}

fn validate_manage_plugin(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &[
            "action",
            "plugin_id",
            "command_id",
            "arguments",
            "remove_storage",
            "package_url",
            "checksum",
            "overwrite",
            "setting_id",
            "value",
        ],
        &["action"],
    )?;
    let action = required_enum(
        object,
        "action",
        &[
            "install",
            "enable",
            "disable",
            "configure",
            "uninstall",
            "invoke",
            "open_manager",
        ],
    )?;
    if action == "open_manager" {
        return Ok(());
    }
    if action == "install" {
        required_string(object, "package_url", MAX_URL_CHARS)?;
        optional_string(object, "checksum", 256)?;
        optional_bool(object, "overwrite")?;
        return Ok(());
    }
    required_string(object, "plugin_id", MAX_IDENTIFIER_CHARS)?;
    match action {
        "invoke" => {
            required_string(object, "command_id", MAX_IDENTIFIER_CHARS)?;
            object
                .get("arguments")
                .and_then(Value::as_object)
                .map(|_| ())
                .ok_or(OrchestratorArgumentError::InvalidArguments)
        }
        "configure" => {
            required_string(object, "setting_id", MAX_IDENTIFIER_CHARS)?;
            object
                .contains_key("value")
                .then_some(())
                .ok_or(OrchestratorArgumentError::InvalidArguments)
        }
        "uninstall" => optional_bool(object, "remove_storage"),
        _ => Ok(()),
    }
}

fn validate_manage_credential(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_fields(
        object,
        &["action", "kind", "id", "connection_id", "force"],
        &["action"],
    )?;
    let action = required_enum(object, "action", &["open_manager", "delete"])?;
    if action == "open_manager" {
        return Ok(());
    }
    let kind = required_enum(
        object,
        "kind",
        &["managed_ssh_key", "privilege", "remote_desktop"],
    )?;
    required_string(object, "id", MAX_IDENTIFIER_CHARS)?;
    optional_bool(object, "force")?;
    if kind == "privilege" {
        required_string(object, "connection_id", MAX_IDENTIFIER_CHARS)?;
    } else if object.contains_key("connection_id") {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn validate_exact_authority(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    match (
        object.contains_key("handle_id"),
        object.contains_key("resource_ref"),
    ) {
        (true, false) => required_string(object, "handle_id", 64).map(|_| ()),
        (false, true) => validate_saved_connection_ref(
            object
                .get("resource_ref")
                .ok_or(OrchestratorArgumentError::InvalidArguments)?,
        ),
        _ => Err(OrchestratorArgumentError::InvalidArguments),
    }
}

fn validate_saved_connection_ref(value: &Value) -> Result<(), OrchestratorArgumentError> {
    let object = value
        .as_object()
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    require_fields(object, &["kind", "id", "label"], &["kind", "id"])?;
    if required_string(object, "kind", 32)? != "saved_connection" {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    required_string(object, "id", MAX_IDENTIFIER_CHARS).map(|_| ())
}

fn tool(name: &str, description: &str, parameters: Value) -> AiToolDefinition {
    AiToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

fn empty_object_schema() -> Value {
    json!({ "type": "object", "properties": {}, "additionalProperties": false })
}

fn id_schema(field: &str) -> Value {
    json!({
        "type": "object",
        "properties": { (field): { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS } },
        "required": [(field)],
        "additionalProperties": false
    })
}

fn authority_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "handle_id": { "type": "string", "maxLength": 64 },
            "resource_ref": {
                "type": "object",
                "properties": {
                    "kind": { "const": "saved_connection" },
                    "id": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS },
                    "label": { "type": "string", "maxLength": MAX_IDENTIFIER_CHARS }
                },
                "required": ["kind", "id"],
                "additionalProperties": false
            }
        },
        "oneOf": [
            { "required": ["handle_id"] },
            { "required": ["resource_ref"] }
        ],
        "additionalProperties": false
    })
}

fn require_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    required: &[&str],
) -> Result<(), OrchestratorArgumentError> {
    if !object.keys().all(|key| allowed.contains(&key.as_str()))
        || !required.iter().all(|key| object.contains_key(*key))
    {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    maximum_chars: usize,
) -> Result<&'a str, OrchestratorArgumentError> {
    let value = object
        .get(field)
        .and_then(Value::as_str)
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    if value.trim().is_empty() || value.chars().count() > maximum_chars {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(value)
}

fn optional_string(
    object: &Map<String, Value>,
    field: &str,
    maximum_chars: usize,
) -> Result<(), OrchestratorArgumentError> {
    if object.contains_key(field) {
        required_string(object, field, maximum_chars)?;
    }
    Ok(())
}

fn optional_string_allow_empty(
    object: &Map<String, Value>,
    field: &str,
    maximum_chars: usize,
) -> Result<(), OrchestratorArgumentError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    let value = value
        .as_str()
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    if value.chars().count() > maximum_chars {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn optional_string_array(
    object: &Map<String, Value>,
    field: &str,
    maximum_items: usize,
    maximum_item_chars: usize,
) -> Result<(), OrchestratorArgumentError> {
    let Some(values) = object.get(field) else {
        return Ok(());
    };
    let values = values
        .as_array()
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    if values.len() > maximum_items
        || values.iter().any(|value| {
            value.as_str().is_none_or(|value| {
                value.trim().is_empty() || value.chars().count() > maximum_item_chars
            })
        })
    {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn optional_positive_number(
    object: &Map<String, Value>,
    field: &str,
) -> Result<(), OrchestratorArgumentError> {
    let Some(value) = object.get(field) else {
        return Ok(());
    };
    let value = value
        .as_f64()
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    if !value.is_finite() || value <= 0.0 {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn required_enum<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    allowed: &[&str],
) -> Result<&'a str, OrchestratorArgumentError> {
    let value = required_string(object, field, 64)?;
    allowed
        .contains(&value)
        .then_some(value)
        .ok_or(OrchestratorArgumentError::InvalidArguments)
}

fn optional_enum(
    object: &Map<String, Value>,
    field: &str,
    allowed: &[&str],
) -> Result<(), OrchestratorArgumentError> {
    if object.contains_key(field) {
        required_enum(object, field, allowed)?;
    }
    Ok(())
}

fn optional_bool(
    object: &Map<String, Value>,
    field: &str,
) -> Result<(), OrchestratorArgumentError> {
    if object.contains_key(field) && object.get(field).and_then(Value::as_bool).is_none() {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn required_bool(
    object: &Map<String, Value>,
    field: &str,
) -> Result<(), OrchestratorArgumentError> {
    if object.get(field).and_then(Value::as_bool).is_none() {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn required_integer(
    object: &Map<String, Value>,
    field: &str,
    minimum: u64,
    maximum: u64,
) -> Result<(), OrchestratorArgumentError> {
    let value = object
        .get(field)
        .and_then(Value::as_u64)
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    if !(minimum..=maximum).contains(&value) {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn optional_integer(
    object: &Map<String, Value>,
    field: &str,
    minimum: u64,
    maximum: u64,
) -> Result<(), OrchestratorArgumentError> {
    if object.contains_key(field) {
        required_integer(object, field, minimum, maximum)?;
    }
    Ok(())
}

fn bounded_json(value: &Value, nodes: &mut usize) -> bool {
    if *nodes == 0 {
        return false;
    }
    *nodes -= 1;
    match value {
        Value::String(value) => value.chars().count() <= MAX_TASK_ARGUMENT_STRING_CHARS,
        Value::Array(values) => values.iter().all(|value| bounded_json(value, nodes)),
        Value::Object(values) => values
            .iter()
            .all(|(key, value)| key.chars().count() <= 256 && bounded_json(value, nodes)),
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
    }
}

fn contains_live_handle(object: &Map<String, Value>) -> bool {
    object.iter().any(|(key, value)| {
        matches!(
            key.as_str(),
            "handle_id" | "handleId" | "session_id" | "sessionId"
        ) || value.as_object().is_some_and(contains_live_handle)
            || value.as_array().is_some_and(|values| {
                values
                    .iter()
                    .any(|value| value.as_object().is_some_and(contains_live_handle))
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recurring_task_rejects_nested_live_handle() {
        let arguments = json!({
            "tool_name": "inspect_host_tools",
            "arguments": { "nested": { "handle_id": "turn-handle" } },
            "mode": "interval",
            "interval_seconds": 30,
            "max_runs": 10
        });

        assert_eq!(
            validate_extended_application_tool_arguments(
                "create_background_task",
                arguments.as_object().expect("object"),
            ),
            Err(OrchestratorArgumentError::InvalidArguments)
        );
    }

    #[test]
    fn serial_controls_require_the_action_specific_value() {
        let missing_state = json!({
            "handle_id": "terminal-handle",
            "action": "set_dtr"
        });
        assert_eq!(
            validate_extended_application_tool_arguments(
                "manage_serial_session",
                missing_state.as_object().expect("object"),
            ),
            Err(OrchestratorArgumentError::InvalidArguments)
        );

        let valid_line_ending = json!({
            "handle_id": "terminal-handle",
            "action": "set_line_ending",
            "value": "crlf"
        });
        assert_eq!(
            validate_extended_application_tool_arguments(
                "manage_serial_session",
                valid_line_ending.as_object().expect("object"),
            ),
            Ok(true)
        );
    }

    #[test]
    fn telnet_controls_accept_only_declared_protocol_actions() {
        let valid = json!({
            "handle_id": "terminal-handle",
            "action": "interrupt_process"
        });
        assert_eq!(
            validate_extended_application_tool_arguments(
                "manage_telnet_session",
                valid.as_object().expect("object"),
            ),
            Ok(true)
        );

        let invalid = json!({
            "handle_id": "terminal-handle",
            "action": "send_raw_iac"
        });
        assert_eq!(
            validate_extended_application_tool_arguments(
                "manage_telnet_session",
                invalid.as_object().expect("object"),
            ),
            Err(OrchestratorArgumentError::InvalidArguments)
        );
    }

    #[test]
    fn memory_update_rejects_scope_identity_without_scope_kind() {
        let arguments = json!({
            "action": "update",
            "id": "memory-1",
            "scope_id": "host-a",
            "expected_revision": 1
        });

        assert_eq!(
            validate_extended_application_tool_arguments(
                "manage_memory_entry",
                arguments.as_object().expect("object"),
            ),
            Err(OrchestratorArgumentError::InvalidArguments)
        );
    }
}
