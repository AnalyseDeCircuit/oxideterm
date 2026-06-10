use serde_json::json;

use crate::AiToolDefinition;

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
const RESOURCE_KIND_ENUM: &[&str] = &["settings", "file", "directory", "sftp", "ide", "rag"];

pub fn orchestrator_tool_definitions() -> Vec<AiToolDefinition> {
    vec![
        tool(
            "list_targets",
            "List available OxideTerm targets by view. Default view is connections for remote host discovery. Use view=all only for debugging or last-resort fallback.",
            json!({
                "type": "object",
                "properties": {
                    "view": { "type": "string", "enum": TARGET_VIEW_ENUM, "description": "Target view. Default: connections. Use connections for remote hosts; live_sessions for active shells/SFTP; app_surfaces for settings/UI; files for file-capable targets; all only for debug/fallback." },
                    "query": { "type": "string", "description": "Optional filter text. Leave empty for broad discovery." },
                    "kind": { "type": "string", "enum": TARGET_KIND_ENUM, "description": "Optional legacy/fine-grained target kind filter. Prefer view for normal discovery." },
                },
            }),
        ),
        tool(
            "select_target",
            "Select exactly one target from OxideTerm targets. Use only when the user named a specific target. Do not use for broad list/discovery requests.",
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Specific target name, host, user, session label, tab, or settings area." },
                    "intent": { "type": "string", "enum": TARGET_INTENT_ENUM, "description": "Required intended operation. Use knowledge for RAG/knowledge-base/runbook/documentation queries. This constrains the candidate pool so commands are not mistaken for targets." },
                    "kind": { "type": "string", "enum": TARGET_KIND_ENUM, "description": "Optional target kind filter." },
                },
                "required": ["query", "intent"],
            }),
        ),
        tool(
            "connect_target",
            "Connect or open a selected target. For saved SSH connections, opens the saved connection through OxideTerm and returns live ssh-node and terminal-session targets.",
            json!({
                "type": "object",
                "properties": {
                    "target_id": { "type": "string", "description": "Target ID from list_targets/select_target, usually saved-connection:*." },
                },
                "required": ["target_id"],
            }),
        ),
        tool(
            "run_command",
            "Run a command on an explicit target. In the native app, ssh-node:* and terminal-session:* commands are sent through a visible terminal; saved connections must be connected first.",
            json!({
                "type": "object",
                "properties": {
                    "target_id": { "type": "string", "description": "Explicit target ID. Saved connections must be connected first." },
                    "command": { "type": "string", "description": "Shell command to run." },
                    "cwd": { "type": "string", "description": "Optional working directory." },
                    "timeout_secs": { "type": "number", "minimum": 1, "maximum": 60, "description": "Timeout for direct/local command execution. Default: 30." },
                    "await_output": { "type": "boolean", "description": "For terminal-session targets, wait for output. Default: true." },
                },
                "required": ["target_id", "command"],
            }),
        ),
        tool(
            "observe_terminal",
            "Read a terminal target screen, buffer, readiness, and waiting-for-input hints. Use after run_command or before interactive input.",
            json!({
                "type": "object",
                "properties": {
                    "target_id": { "type": "string", "description": "terminal-session:* target ID." },
                    "max_chars": { "type": "number", "minimum": 200, "maximum": 12000, "description": "Maximum returned buffer characters. Default: 4000." },
                },
                "required": ["target_id"],
            }),
        ),
        tool(
            "send_terminal_input",
            "Send literal interactive text or Enter to a visible terminal target after observing a prompt. Do not use this to run shell commands; use run_command instead. Control sequences such as Ctrl-C are not supported here.",
            json!({
                "type": "object",
                "properties": {
                    "target_id": { "type": "string", "description": "terminal-session:* target ID." },
                    "text": { "type": "string", "description": "Text to send." },
                    "append_enter": { "type": "boolean", "description": "Append Enter after text. Default: false." },
                },
                "required": ["target_id"],
            }),
        ),
        tool(
            "read_resource",
            "Read a resource from a target: settings section, remote file via agent/SFTP, SFTP directory, IDE file, or RAG search.",
            json!({
                "type": "object",
                "properties": {
                    "target_id": { "type": "string", "description": "Target ID." },
                    "resource": { "type": "string", "enum": RESOURCE_KIND_ENUM, "description": "Resource kind." },
                    "path": { "type": "string", "description": "File or directory path when applicable." },
                    "section": { "type": "string", "description": "Settings section when resource=settings." },
                    "query": { "type": "string", "description": "Search query for RAG or target-specific searches. For resource=rag, pass target_id=\"rag-index:default\" plus query; path is not required." },
                },
                "required": ["target_id", "resource"],
            }),
        ),
        tool(
            "write_resource",
            "Safely write a resource such as a settings value or remote file. For file edits, provide expected_hash or dry_run unless the user explicitly asked to overwrite.",
            json!({
                "type": "object",
                "properties": {
                    "target_id": { "type": "string", "description": "Target ID." },
                    "resource": { "type": "string", "enum": RESOURCE_KIND_ENUM, "description": "Resource kind. Only settings and file are writable." },
                    "section": { "type": "string", "description": "Settings section." },
                    "key": { "type": "string", "description": "Settings key." },
                    "value": { "description": "Settings value or structured resource value." },
                    "path": { "type": "string", "description": "Remote file path." },
                    "content": { "type": "string", "description": "File content." },
                    "expected_hash": { "type": "string", "description": "Hash from prior read_resource result." },
                    "dry_run": { "type": "boolean", "description": "Validate without writing." },
                },
                "required": ["target_id", "resource"],
            }),
        ),
        tool(
            "transfer_resource",
            "Start an SFTP upload/download/transfer against an explicit SSH/SFTP target.",
            json!({
                "type": "object",
                "properties": {
                    "target_id": { "type": "string", "description": "ssh-node:* or sftp-session:* target ID." },
                    "direction": { "type": "string", "enum": ["upload", "download"], "description": "Transfer direction." },
                    "source_path": { "type": "string", "description": "Local path for upload or remote path for download." },
                    "destination_path": { "type": "string", "description": "Remote path for upload or local path for download." },
                },
                "required": ["target_id", "direction", "source_path", "destination_path"],
            }),
        ),
        tool(
            "open_app_surface",
            "Open an OxideTerm app surface such as settings, connection manager, SFTP, IDE, file manager, or local terminal.",
            json!({
                "type": "object",
                "properties": {
                    "surface": { "type": "string", "enum": ["settings", "connection_manager", "connection_pool", "connection_monitor", "sftp", "ide", "file_manager", "local_terminal", "terminal"], "description": "Surface to open." },
                    "target_id": { "type": "string", "description": "Optional target to open the surface for." },
                    "section": { "type": "string", "description": "Optional settings section." },
                },
                "required": ["surface"],
            }),
        ),
        tool(
            "get_state",
            "Read compact state: connection status, transfer status, settings summary, active targets, or health. Use for diagnostics and verification.",
            json!({
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "enum": ["connections", "transfers", "settings", "targets", "health", "active"], "description": "State scope." },
                    "target_id": { "type": "string", "description": "Optional target ID." },
                },
                "required": ["scope"],
            }),
        ),
        tool(
            "remember_preference",
            "Save a long-lived user preference for OxideSens memory. Do not use for transient task facts.",
            json!({
                "type": "object",
                "properties": {
                    "preference": { "type": "string", "description": "Preference to remember." },
                },
                "required": ["preference"],
            }),
        ),
        tool(
            "recall_preferences",
            "Read saved long-lived OxideSens user preferences.",
            json!({
                "type": "object",
                "properties": {},
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
