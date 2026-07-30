use serde_json::{Map, Value, json};
use thiserror::Error;

use crate::{AiToolDefinition, StableResourceKind, StableResourceRef};

const TARGET_KIND_ENUM: &[&str] = &[
    "all",
    "saved-connection",
    "ssh-node",
    "terminal-session",
    "local-shell",
    "sftp-session",
    "ide-workspace",
    "settings",
    "app-surface",
    "rag-index",
];
const TARGET_VIEW_ENUM: &[&str] = &[
    "connections",
    "live_sessions",
    "app_surfaces",
    "files",
    "all",
];
const TARGET_INTENT_ENUM: &[&str] = &[
    "connection",
    "command",
    "terminal",
    "settings",
    "file",
    "sftp",
    "app_surface",
    "knowledge",
    "status",
    "local",
    "unknown",
];
const READ_RESOURCE_KIND_ENUM: &[&str] = &["settings", "rag", "file", "directory", "sftp", "ide"];
const WRITE_RESOURCE_KIND_ENUM: &[&str] = &["settings", "file", "ide"];
const APP_SURFACE_KIND_ENUM: &[&str] = &[
    "settings",
    "connection_manager",
    "connection_pool",
    "connection_monitor",
    "sftp",
    "ide",
    "file_manager",
    "local_terminal",
    "terminal",
];
const MAX_QUERY_CHARS: usize = 4_096;
const MAX_COMMAND_CHARS: usize = 65_536;
const MAX_PATH_CHARS: usize = 8_192;
const MAX_TERMINAL_INPUT_CHARS: usize = 65_536;
const MAX_FILE_CONTENT_CHARS: usize = 2_000_000;
const MAX_SETTINGS_COMPONENT_CHARS: usize = 256;
const MAX_SETTINGS_VALUE_STRING_CHARS: usize = 65_536;
const MAX_SETTINGS_VALUE_NODES: usize = 4_096;
const MAX_CONTENT_HASH_CHARS: usize = 256;
const MAX_PREFERENCE_CHARS: usize = 12_000;

pub fn orchestrator_tool_definitions() -> Vec<AiToolDefinition> {
    vec![
        tool(
            "list_targets",
            "List available OxideTerm targets by view. Default view is connections for remote host discovery. Use view=all only for debugging or last-resort fallback.",
            json!({
                "type": "object",
                "properties": {
                    "view": { "type": "string", "enum": TARGET_VIEW_ENUM, "description": "Target view. Default: connections. Use connections for remote hosts; live_sessions for active shells/SFTP; app_surfaces for settings/UI; files for file-capable targets; all only for debug/fallback." },
                    "query": { "type": "string", "maxLength": MAX_QUERY_CHARS, "description": "Optional filter text. Leave empty for broad discovery." },
                    "kind": { "type": "string", "enum": TARGET_KIND_ENUM, "description": "Optional fine-grained target kind filter. Prefer view for normal discovery." },
                },
                "additionalProperties": false,
            }),
        ),
        tool(
            "select_target",
            "Select exactly one target from OxideTerm targets. Use only when the user named a specific target. Do not use for broad list/discovery requests.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "maxLength": MAX_QUERY_CHARS, "description": "Specific target name, host, user, session label, tab, or settings area." },
                    "intent": { "type": "string", "enum": TARGET_INTENT_ENUM, "description": "Required intended operation. Use knowledge for RAG/knowledge-base/runbook/documentation queries. This constrains the candidate pool so commands are not mistaken for targets." },
                    "kind": { "type": "string", "enum": TARGET_KIND_ENUM, "description": "Optional target kind filter." },
                },
                "required": ["query", "intent"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "connect_target",
            "Connect a saved SSH connection selected from discovery. The resource_ref proves durable identity only; rediscover the resulting live terminal handle before running commands.",
            json!({
                "type": "object",
                "properties": {
                    "resource_ref": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["saved_connection"] },
                            "id": { "type": "string", "format": "uuid" },
                            "label": { "type": "string", "maxLength": 256 },
                        },
                        "required": ["kind", "id"],
                        "additionalProperties": false,
                        "description": "Saved connection reference returned by discovery.",
                    },
                },
                "required": ["resource_ref"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "run_command",
            "Run a command through a current terminal handle returned by list_targets or select_target. Saved connections must be connected first, then discover the resulting terminal handle.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64, "description": "Current terminal handle from list_targets/select_target." },
                    "command": { "type": "string", "maxLength": MAX_COMMAND_CHARS, "description": "Shell command to run." },
                    "cwd": { "type": "string", "maxLength": MAX_PATH_CHARS, "description": "Optional working directory." },
                    "timeout_secs": { "type": "number", "minimum": 1, "maximum": 60, "description": "Timeout for direct/local command execution. Default: 30." },
                    "await_output": { "type": "boolean", "description": "For terminal-session targets, wait for output. Default: true." },
                },
                "required": ["handle_id", "command"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "observe_terminal",
            "Read a terminal target screen, buffer, readiness, and waiting-for-input hints. Use after run_command or before interactive input.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64, "description": "Current terminal handle from list_targets/select_target." },
                    "max_chars": { "type": "number", "minimum": 200, "maximum": 12000, "description": "Maximum returned buffer characters. Default: 4000." },
                },
                "required": ["handle_id"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "send_terminal_input",
            "Send literal interactive text or Enter to a visible terminal target after observing a prompt. Do not use this to run shell commands; use run_command instead. Control sequences such as Ctrl-C are not supported here.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64, "description": "Current terminal handle from list_targets/select_target." },
                    "text": { "type": "string", "maxLength": MAX_TERMINAL_INPUT_CHARS, "description": "Text to send." },
                    "append_enter": { "type": "boolean", "description": "Append Enter after text. Default: false." },
                },
                "required": ["handle_id"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "read_resource",
            "Read a durable OxideTerm resource or a live remote resource. Settings and knowledge use resource_ref; remote files, directories, SFTP, and IDE files require a current handle_id.",
            json!({
                "type": "object",
                "properties": {
                    "resource_ref": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["settings_scope", "rag_index"] },
                            "id": { "type": "string", "enum": ["app", "default"] },
                            "label": { "type": "string", "maxLength": 256 },
                        },
                        "required": ["kind", "id"],
                        "additionalProperties": false,
                        "description": "Stable resource reference returned by discovery.",
                    },
                    "handle_id": { "type": "string", "maxLength": 64, "description": "Current SFTP or IDE handle for live resources." },
                    "resource": { "type": "string", "enum": READ_RESOURCE_KIND_ENUM, "description": "Resource kind." },
                    "path": { "type": "string", "maxLength": MAX_PATH_CHARS, "description": "Remote file or directory path for live resources." },
                    "section": { "type": "string", "maxLength": MAX_SETTINGS_COMPONENT_CHARS, "description": "Settings section when resource=settings." },
                    "query": { "type": "string", "maxLength": MAX_QUERY_CHARS, "description": "Search query when resource=rag." },
                },
                "required": ["resource"],
                "oneOf": [
                    {
                        "properties": { "resource": { "const": "settings" } },
                        "required": ["resource_ref"],
                        "not": { "anyOf": [
                            { "required": ["handle_id"] },
                            { "required": ["path"] },
                            { "required": ["query"] }
                        ]}
                    },
                    {
                        "properties": { "resource": { "const": "rag" } },
                        "required": ["resource_ref", "query"],
                        "not": { "anyOf": [
                            { "required": ["handle_id"] },
                            { "required": ["path"] },
                            { "required": ["section"] }
                        ]}
                    },
                    {
                        "properties": {
                            "resource": { "enum": ["file", "directory", "sftp", "ide"] }
                        },
                        "required": ["handle_id", "path"],
                        "not": { "anyOf": [
                            { "required": ["resource_ref"] },
                            { "required": ["section"] },
                            { "required": ["query"] }
                        ]}
                    }
                ],
                "additionalProperties": false,
            }),
        ),
        tool(
            "write_resource",
            "Safely write a durable OxideTerm settings value or a live remote file. Settings use resource_ref; remote files and IDE files require a current handle_id.",
            json!({
                "type": "object",
                "properties": {
                    "resource_ref": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["settings_scope"] },
                            "id": { "type": "string", "enum": ["app"] },
                            "label": { "type": "string", "maxLength": 256 },
                        },
                        "required": ["kind", "id"],
                        "additionalProperties": false,
                        "description": "Application settings scope reference returned by discovery.",
                    },
                    "handle_id": { "type": "string", "maxLength": 64, "description": "Current SFTP or IDE handle for live file writes." },
                    "resource": { "type": "string", "enum": WRITE_RESOURCE_KIND_ENUM, "description": "Writable resource kind." },
                    "path": { "type": "string", "maxLength": MAX_PATH_CHARS, "description": "Remote file path for live writes." },
                    "content": { "type": "string", "maxLength": MAX_FILE_CONTENT_CHARS, "description": "Complete replacement content for a live file write." },
                    "expected_hash": { "type": "string", "maxLength": MAX_CONTENT_HASH_CHARS, "description": "Optional content hash used to reject a stale overwrite." },
                    "section": { "type": "string", "maxLength": MAX_SETTINGS_COMPONENT_CHARS, "description": "Settings section." },
                    "key": { "type": "string", "maxLength": MAX_SETTINGS_COMPONENT_CHARS, "description": "Settings key." },
                    "value": { "description": "Settings value or structured resource value." },
                    "dry_run": { "type": "boolean", "description": "Validate without writing." },
                },
                "required": ["resource"],
                "oneOf": [
                    {
                        "properties": { "resource": { "const": "settings" } },
                        "required": ["resource_ref", "section", "key", "value"],
                        "not": { "anyOf": [
                            { "required": ["handle_id"] },
                            { "required": ["path"] },
                            { "required": ["content"] },
                            { "required": ["expected_hash"] }
                        ]}
                    },
                    {
                        "properties": { "resource": { "enum": ["file", "ide"] } },
                        "required": ["handle_id", "path", "content"],
                        "not": { "anyOf": [
                            { "required": ["resource_ref"] },
                            { "required": ["section"] },
                            { "required": ["key"] },
                            { "required": ["value"] }
                        ]}
                    }
                ],
                "additionalProperties": false,
            }),
        ),
        tool(
            "transfer_resource",
            "Start an SFTP transfer with a current SFTP handle. The capability is exposed only after a concrete SFTP owner is available.",
            json!({
                "type": "object",
                "properties": {
                    "handle_id": { "type": "string", "maxLength": 64, "description": "Current SFTP handle from discovery." },
                    "direction": { "type": "string", "enum": ["upload", "download"], "description": "Transfer direction." },
                    "source_path": { "type": "string", "maxLength": MAX_PATH_CHARS, "description": "Local path for upload or remote path for download." },
                    "destination_path": { "type": "string", "maxLength": MAX_PATH_CHARS, "description": "Remote path for upload or local path for download." },
                },
                "required": ["handle_id", "direction", "source_path", "destination_path"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "open_app_surface",
            "Open an OxideTerm application surface by durable resource_ref, or focus one exact mounted surface by its current handle_id.",
            json!({
                "type": "object",
                "properties": {
                    "resource_ref": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["app_surface"] },
                            "id": { "type": "string", "enum": APP_SURFACE_KIND_ENUM },
                            "label": { "type": "string", "maxLength": 256 },
                        },
                        "required": ["kind", "id"],
                        "additionalProperties": false,
                        "description": "Durable application surface reference returned by discovery.",
                    },
                    "handle_id": { "type": "string", "maxLength": 64, "description": "Current mounted-surface handle returned by discovery." },
                    "section": { "type": "string", "maxLength": MAX_SETTINGS_COMPONENT_CHARS, "description": "Optional settings section." },
                },
                "oneOf": [
                    { "required": ["resource_ref"] },
                    { "required": ["handle_id"] }
                ],
                "additionalProperties": false,
            }),
        ),
        tool(
            "get_state",
            "Read compact global state, or inspect one exact live handle or durable resource reference without exposing internal runtime identifiers.",
            json!({
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "enum": ["connections", "transfers", "settings", "targets", "health", "active", "target"], "description": "State scope. Use target with exactly one authority field." },
                    "handle_id": { "type": "string", "maxLength": 64, "description": "Current live target handle when scope=target." },
                    "resource_ref": {
                        "type": "object",
                        "properties": {
                            "kind": { "type": "string", "enum": ["saved_connection", "local_shell_profile", "settings_scope", "rag_index", "app_surface"] },
                            "id": { "type": "string", "maxLength": 160 },
                            "label": { "type": "string", "maxLength": 256 },
                        },
                        "required": ["kind", "id"],
                        "additionalProperties": false,
                        "description": "Durable target reference when scope=target.",
                    },
                },
                "required": ["scope"],
                "allOf": [{
                    "if": {
                        "properties": { "scope": { "const": "target" } },
                        "required": ["scope"]
                    },
                    "then": {
                        "oneOf": [
                            { "required": ["handle_id"] },
                            { "required": ["resource_ref"] }
                        ]
                    },
                    "else": {
                        "not": {
                            "anyOf": [
                                { "required": ["handle_id"] },
                                { "required": ["resource_ref"] }
                            ]
                        }
                    }
                }],
                "additionalProperties": false,
            }),
        ),
        tool(
            "remember_preference",
            "Save a long-lived user preference for OxideSens memory. Do not use for transient task facts.",
            json!({
                "type": "object",
                "properties": {
                    "preference": { "type": "string", "maxLength": MAX_PREFERENCE_CHARS, "description": "Preference to remember." },
                },
                "required": ["preference"],
                "additionalProperties": false,
            }),
        ),
        tool(
            "recall_preferences",
            "Read saved long-lived OxideSens user preferences.",
            json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false,
            }),
        ),
    ]
}

fn tool(name: &str, description: &str, parameters: serde_json::Value) -> AiToolDefinition {
    AiToolDefinition {
        name: name.to_string(),
        description: description.to_string(),
        parameters,
    }
}

/// A safe parse failure that never includes submitted command, input, or file content.
#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum OrchestratorArgumentError {
    #[error("unknown application tool")]
    UnknownTool,
    #[error("invalid application tool arguments")]
    InvalidArguments,
}

/// Enforces the v2 contract before policy or approval sees an application tool call.
///
/// Returning the original value keeps the canonical policy and execution object identical
/// without serializing secret-capable fields into a second representation.
pub fn canonicalize_orchestrator_tool_arguments(
    tool_name: &str,
    arguments: Value,
) -> Result<Value, OrchestratorArgumentError> {
    let object = arguments
        .as_object()
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    match tool_name {
        "list_targets" => validate_list_targets(object)?,
        "select_target" => validate_select_target(object)?,
        "connect_target" => validate_connect_target(object)?,
        "run_command" => validate_run_command(object)?,
        "observe_terminal" => validate_observe_terminal(object)?,
        "send_terminal_input" => validate_send_terminal_input(object)?,
        "read_resource" => validate_read_resource(object)?,
        "write_resource" => validate_write_resource(object)?,
        "transfer_resource" => validate_transfer_resource(object)?,
        "open_app_surface" => validate_open_app_surface(object)?,
        "get_state" => validate_get_state(object)?,
        "remember_preference" => validate_remember_preference(object)?,
        "recall_preferences" => require_only_fields(object, &[])?,
        _ => return Err(OrchestratorArgumentError::UnknownTool),
    }
    Ok(arguments)
}

fn validate_list_targets(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(object, &["view", "query", "kind"])?;
    optional_enum(object, "view", TARGET_VIEW_ENUM)?;
    optional_string(object, "query", Some(MAX_QUERY_CHARS))?;
    optional_enum(object, "kind", TARGET_KIND_ENUM)
}

fn validate_select_target(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(object, &["query", "intent", "kind"])?;
    required_non_empty_string(object, "query", Some(MAX_QUERY_CHARS))?;
    required_enum(object, "intent", TARGET_INTENT_ENUM)?;
    optional_enum(object, "kind", TARGET_KIND_ENUM)
}

fn validate_connect_target(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(object, &["resource_ref"])?;
    required_resource_ref(object, StableResourceKind::SavedConnection, None)
}

fn validate_run_command(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(
        object,
        &[
            "handle_id",
            "command",
            "cwd",
            "timeout_secs",
            "await_output",
        ],
    )?;
    required_non_empty_string(object, "handle_id", Some(64))?;
    required_non_empty_string(object, "command", Some(MAX_COMMAND_CHARS))?;
    optional_string(object, "cwd", Some(MAX_PATH_CHARS))?;
    optional_u64_in_range(object, "timeout_secs", 1, 60)?;
    optional_bool(object, "await_output")
}

fn validate_observe_terminal(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(object, &["handle_id", "max_chars"])?;
    required_non_empty_string(object, "handle_id", Some(64))?;
    optional_u64_in_range(object, "max_chars", 200, 12_000)
}

fn validate_send_terminal_input(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(object, &["handle_id", "text", "append_enter"])?;
    required_non_empty_string(object, "handle_id", Some(64))?;
    optional_string(object, "text", Some(MAX_TERMINAL_INPUT_CHARS))?;
    optional_bool(object, "append_enter")
}

fn validate_read_resource(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    let resource = required_enum(object, "resource", READ_RESOURCE_KIND_ENUM)?;
    match resource {
        "settings" => {
            require_only_fields(object, &["resource", "resource_ref", "section"])?;
            required_resource_ref(object, StableResourceKind::SettingsScope, Some("app"))?;
            optional_string(object, "section", Some(MAX_SETTINGS_COMPONENT_CHARS))
        }
        "rag" => {
            require_only_fields(object, &["resource", "resource_ref", "query"])?;
            required_resource_ref(object, StableResourceKind::RagIndex, Some("default"))?;
            required_non_empty_string(object, "query", Some(MAX_QUERY_CHARS)).map(|_| ())
        }
        "file" | "directory" | "sftp" | "ide" => {
            require_only_fields(object, &["resource", "handle_id", "path"])?;
            required_non_empty_string(object, "handle_id", Some(64))?;
            required_non_empty_string(object, "path", Some(MAX_PATH_CHARS)).map(|_| ())
        }
        _ => Err(OrchestratorArgumentError::InvalidArguments),
    }
}

fn validate_write_resource(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    let resource = required_enum(object, "resource", WRITE_RESOURCE_KIND_ENUM)?;
    match resource {
        "settings" => {
            require_only_fields(
                object,
                &[
                    "resource",
                    "resource_ref",
                    "section",
                    "key",
                    "value",
                    "dry_run",
                ],
            )?;
            required_resource_ref(object, StableResourceKind::SettingsScope, Some("app"))?;
            required_non_empty_string(object, "section", Some(MAX_SETTINGS_COMPONENT_CHARS))?;
            required_non_empty_string(object, "key", Some(MAX_SETTINGS_COMPONENT_CHARS))?;
            let Some(value) = object.get("value") else {
                return Err(OrchestratorArgumentError::InvalidArguments);
            };
            let mut remaining_nodes = MAX_SETTINGS_VALUE_NODES;
            if !structured_value_is_bounded(value, &mut remaining_nodes) {
                return Err(OrchestratorArgumentError::InvalidArguments);
            }
            optional_bool(object, "dry_run")
        }
        "file" | "ide" => {
            require_only_fields(
                object,
                &[
                    "resource",
                    "handle_id",
                    "path",
                    "content",
                    "expected_hash",
                    "dry_run",
                ],
            )?;
            required_non_empty_string(object, "handle_id", Some(64))?;
            required_non_empty_string(object, "path", Some(MAX_PATH_CHARS))?;
            required_string(object, "content", Some(MAX_FILE_CONTENT_CHARS))?;
            optional_string(object, "expected_hash", Some(MAX_CONTENT_HASH_CHARS))?;
            optional_bool(object, "dry_run")
        }
        _ => Err(OrchestratorArgumentError::InvalidArguments),
    }
}

fn validate_transfer_resource(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(
        object,
        &["handle_id", "direction", "source_path", "destination_path"],
    )?;
    required_non_empty_string(object, "handle_id", Some(64))?;
    required_enum(object, "direction", &["upload", "download"])?;
    required_non_empty_string(object, "source_path", Some(MAX_PATH_CHARS))?;
    required_non_empty_string(object, "destination_path", Some(MAX_PATH_CHARS)).map(|_| ())
}

fn validate_open_app_surface(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(object, &["resource_ref", "handle_id", "section"])?;
    optional_string(object, "section", Some(MAX_SETTINGS_COMPONENT_CHARS))?;
    match (
        object.contains_key("resource_ref"),
        object.contains_key("handle_id"),
    ) {
        (true, false) => required_resource_ref(object, StableResourceKind::AppSurface, None),
        (false, true) => required_non_empty_string(object, "handle_id", Some(64)).map(|_| ()),
        _ => Err(OrchestratorArgumentError::InvalidArguments),
    }
}

fn validate_get_state(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(object, &["scope", "handle_id", "resource_ref"])?;
    let scope = required_enum(
        object,
        "scope",
        &[
            "connections",
            "transfers",
            "settings",
            "targets",
            "health",
            "active",
            "target",
        ],
    )?;
    let has_handle = object.contains_key("handle_id");
    let has_resource_ref = object.contains_key("resource_ref");
    if scope == "target" {
        match (has_handle, has_resource_ref) {
            (true, false) => required_non_empty_string(object, "handle_id", Some(64)).map(|_| ()),
            (false, true) => required_any_resource_ref(object),
            _ => Err(OrchestratorArgumentError::InvalidArguments),
        }
    } else if has_handle || has_resource_ref {
        Err(OrchestratorArgumentError::InvalidArguments)
    } else {
        Ok(())
    }
}

fn validate_remember_preference(
    object: &Map<String, Value>,
) -> Result<(), OrchestratorArgumentError> {
    require_only_fields(object, &["preference"])?;
    required_non_empty_string(object, "preference", Some(MAX_PREFERENCE_CHARS)).map(|_| ())
}

fn require_only_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
) -> Result<(), OrchestratorArgumentError> {
    object
        .keys()
        .all(|key| allowed.contains(&key.as_str()))
        .then_some(())
        .ok_or(OrchestratorArgumentError::InvalidArguments)
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    max_chars: Option<usize>,
) -> Result<&'a str, OrchestratorArgumentError> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(OrchestratorArgumentError::InvalidArguments)?;
    if max_chars.is_some_and(|maximum| value.chars().count() > maximum) {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(value)
}

fn required_non_empty_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    max_chars: Option<usize>,
) -> Result<&'a str, OrchestratorArgumentError> {
    let value = required_string(object, key, max_chars)?;
    (!value.trim().is_empty())
        .then_some(value)
        .ok_or(OrchestratorArgumentError::InvalidArguments)
}

fn optional_string(
    object: &Map<String, Value>,
    key: &str,
    max_chars: Option<usize>,
) -> Result<(), OrchestratorArgumentError> {
    if object.contains_key(key) {
        required_string(object, key, max_chars)?;
    }
    Ok(())
}

fn required_enum<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<&'a str, OrchestratorArgumentError> {
    let value = required_string(object, key, None)?;
    allowed
        .contains(&value)
        .then_some(value)
        .ok_or(OrchestratorArgumentError::InvalidArguments)
}

fn optional_enum(
    object: &Map<String, Value>,
    key: &str,
    allowed: &[&str],
) -> Result<(), OrchestratorArgumentError> {
    if object.contains_key(key) {
        required_enum(object, key, allowed)?;
    }
    Ok(())
}

fn optional_bool(object: &Map<String, Value>, key: &str) -> Result<(), OrchestratorArgumentError> {
    if object.contains_key(key) && object.get(key).and_then(Value::as_bool).is_none() {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn optional_u64_in_range(
    object: &Map<String, Value>,
    key: &str,
    minimum: u64,
    maximum: u64,
) -> Result<(), OrchestratorArgumentError> {
    if let Some(value) = object.get(key) {
        let value = value
            .as_u64()
            .ok_or(OrchestratorArgumentError::InvalidArguments)?;
        if !(minimum..=maximum).contains(&value) {
            return Err(OrchestratorArgumentError::InvalidArguments);
        }
    }
    Ok(())
}

/// Bounds free-form settings JSON before policy evaluation or backend resolution.
fn structured_value_is_bounded(value: &Value, remaining_nodes: &mut usize) -> bool {
    if *remaining_nodes == 0 {
        return false;
    }
    *remaining_nodes -= 1;
    match value {
        Value::String(value) => value.chars().count() <= MAX_SETTINGS_VALUE_STRING_CHARS,
        Value::Array(values) => values
            .iter()
            .all(|value| structured_value_is_bounded(value, remaining_nodes)),
        Value::Object(object) => object.iter().all(|(key, value)| {
            key.chars().count() <= MAX_SETTINGS_COMPONENT_CHARS
                && structured_value_is_bounded(value, remaining_nodes)
        }),
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
    }
}

fn required_resource_ref(
    object: &Map<String, Value>,
    expected_kind: StableResourceKind,
    expected_id: Option<&str>,
) -> Result<(), OrchestratorArgumentError> {
    let resource_ref = parse_resource_ref(object)?;
    if resource_ref.kind() != expected_kind
        || expected_id.is_some_and(|expected_id| resource_ref.id() != expected_id)
    {
        return Err(OrchestratorArgumentError::InvalidArguments);
    }
    Ok(())
}

fn required_any_resource_ref(object: &Map<String, Value>) -> Result<(), OrchestratorArgumentError> {
    parse_resource_ref(object).map(|_| ())
}

fn parse_resource_ref(
    object: &Map<String, Value>,
) -> Result<StableResourceRef, OrchestratorArgumentError> {
    object
        .get("resource_ref")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok())
        .ok_or(OrchestratorArgumentError::InvalidArguments)
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        MAX_COMMAND_CHARS, MAX_CONTENT_HASH_CHARS, MAX_FILE_CONTENT_CHARS, MAX_PATH_CHARS,
        MAX_PREFERENCE_CHARS, MAX_QUERY_CHARS, MAX_SETTINGS_COMPONENT_CHARS,
        MAX_SETTINGS_VALUE_NODES, MAX_SETTINGS_VALUE_STRING_CHARS, MAX_TERMINAL_INPUT_CHARS,
        OrchestratorArgumentError, canonicalize_orchestrator_tool_arguments,
        orchestrator_tool_definitions,
    };

    #[test]
    fn every_free_form_string_schema_has_a_length_bound() {
        fn inspect_schema(value: &Value, path: &str) {
            if let Some(object) = value.as_object() {
                if object.get("type") == Some(&json!("string")) {
                    assert!(
                        object.contains_key("maxLength")
                            || object.contains_key("enum")
                            || object.contains_key("const")
                            || object.contains_key("format"),
                        "unbounded string schema at {path}"
                    );
                }
                for (key, child) in object {
                    inspect_schema(child, &format!("{path}.{key}"));
                }
            } else if let Some(array) = value.as_array() {
                for (index, child) in array.iter().enumerate() {
                    inspect_schema(child, &format!("{path}[{index}]"));
                }
            }
        }

        for tool in orchestrator_tool_definitions() {
            inspect_schema(&tool.parameters, &tool.name);
        }
    }

    #[test]
    fn canonical_parser_rejects_oversized_free_form_fields() {
        fn assert_oversized(tool_name: &str, mut arguments: Value, field: &str, maximum: usize) {
            arguments
                .as_object_mut()
                .expect("argument object")
                .insert(field.to_string(), Value::String("x".repeat(maximum + 1)));
            assert_eq!(
                canonicalize_orchestrator_tool_arguments(tool_name, arguments),
                Err(OrchestratorArgumentError::InvalidArguments),
                "{tool_name}.{field} must be bounded"
            );
        }

        assert_oversized("list_targets", json!({}), "query", MAX_QUERY_CHARS);
        assert_oversized(
            "select_target",
            json!({ "query": "host", "intent": "connection" }),
            "query",
            MAX_QUERY_CHARS,
        );
        assert_oversized(
            "run_command",
            json!({ "handle_id": "rt_current", "command": "pwd" }),
            "command",
            MAX_COMMAND_CHARS,
        );
        assert_oversized(
            "run_command",
            json!({ "handle_id": "rt_current", "command": "pwd" }),
            "cwd",
            MAX_PATH_CHARS,
        );
        assert_oversized(
            "send_terminal_input",
            json!({ "handle_id": "rt_current" }),
            "text",
            MAX_TERMINAL_INPUT_CHARS,
        );
        assert_oversized(
            "read_resource",
            json!({ "resource": "file", "handle_id": "rt_current", "path": "/tmp/a" }),
            "path",
            MAX_PATH_CHARS,
        );
        assert_oversized(
            "read_resource",
            json!({
                "resource": "rag",
                "resource_ref": { "kind": "rag_index", "id": "default" },
                "query": "deployment",
            }),
            "query",
            MAX_QUERY_CHARS,
        );
        assert_oversized(
            "write_resource",
            json!({
                "resource": "file",
                "handle_id": "rt_current",
                "path": "/tmp/a",
                "content": "replacement",
            }),
            "content",
            MAX_FILE_CONTENT_CHARS,
        );
        assert_oversized(
            "write_resource",
            json!({
                "resource": "file",
                "handle_id": "rt_current",
                "path": "/tmp/a",
                "content": "replacement",
            }),
            "expected_hash",
            MAX_CONTENT_HASH_CHARS,
        );
        assert_oversized(
            "transfer_resource",
            json!({
                "handle_id": "rt_current",
                "direction": "upload",
                "source_path": "/tmp/a",
                "destination_path": "/tmp/b",
            }),
            "destination_path",
            MAX_PATH_CHARS,
        );
        assert_oversized(
            "open_app_surface",
            json!({
                "resource_ref": { "kind": "app_surface", "id": "settings" },
            }),
            "section",
            MAX_SETTINGS_COMPONENT_CHARS,
        );
        assert_oversized(
            "remember_preference",
            json!({ "preference": "Use compact layouts." }),
            "preference",
            MAX_PREFERENCE_CHARS,
        );

        let oversized_settings_value = json!({
            "resource": "settings",
            "resource_ref": { "kind": "settings_scope", "id": "app" },
            "section": "terminal",
            "key": "example",
            "value": "x".repeat(MAX_SETTINGS_VALUE_STRING_CHARS + 1),
        });
        assert_eq!(
            canonicalize_orchestrator_tool_arguments("write_resource", oversized_settings_value),
            Err(OrchestratorArgumentError::InvalidArguments)
        );

        let oversized_settings_tree = json!({
            "resource": "settings",
            "resource_ref": { "kind": "settings_scope", "id": "app" },
            "section": "terminal",
            "key": "example",
            "value": vec![Value::Null; MAX_SETTINGS_VALUE_NODES],
        });
        assert_eq!(
            canonicalize_orchestrator_tool_arguments("write_resource", oversized_settings_tree),
            Err(OrchestratorArgumentError::InvalidArguments)
        );
    }
}
