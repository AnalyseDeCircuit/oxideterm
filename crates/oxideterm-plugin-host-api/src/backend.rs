// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, mpsc},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use oxideterm_forwarding::ForwardingRegistry;
use oxideterm_plugin_protocol as plugin_runtime;
use oxideterm_sftp::SftpTransferManager;
use oxideterm_ssh::NodeRouter;
use serde_json::{Value, json};

use crate::{
    app::native_plugin_platform_label, forwarding::native_plugin_forward_response,
    readonly::NativePluginHostApiSnapshot, sftp::native_plugin_sftp_response,
};

pub const NATIVE_PLUGIN_HTTP_BODY_LIMIT: usize = 10 * 1024 * 1024;

pub const NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS: &str = "ssh_get_pool_stats";
pub const NATIVE_PLUGIN_API_COMMAND_LIST_CONNECTIONS: &str = "list_connections";
pub const NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION: &str = "get_app_version";
pub const NATIVE_PLUGIN_API_COMMAND_GET_SYSTEM_INFO: &str = "get_system_info";
pub const NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER: &str = "sftp_cancel_transfer";
pub const NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER: &str = "sftp_pause_transfer";
pub const NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER: &str = "sftp_resume_transfer";
pub const NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS: &str = "sftp_transfer_stats";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT: &str = "node_sftp_init";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR: &str = "node_sftp_list_dir";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT: &str = "node_sftp_stat";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW: &str = "node_sftp_preview";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE: &str = "node_sftp_write";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD: &str = "node_sftp_download";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD: &str = "node_sftp_upload";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR: &str = "node_sftp_mkdir";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE: &str = "node_sftp_delete";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE: &str = "node_sftp_delete_recursive";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME: &str = "node_sftp_rename";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR: &str = "node_sftp_download_dir";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR: &str = "node_sftp_upload_dir";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE: &str = "node_sftp_tar_probe";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD: &str = "node_sftp_tar_upload";
pub const NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD: &str = "node_sftp_tar_download";
pub const NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS: &str = "list_port_forwards";
pub const NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD: &str = "create_port_forward";
pub const NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD: &str = "stop_port_forward";
pub const NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD: &str = "delete_port_forward";
pub const NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD: &str = "restart_port_forward";
pub const NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD: &str = "update_port_forward";
pub const NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS: &str = "get_port_forward_stats";
pub const NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS: &str = "stop_all_forwards";
pub const NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST: &str = "plugin_http_request";

// Keep the documented api.invoke adapter surface in one place so tests can
// detect a command that is listed but not dispatched through a native owner.
pub fn native_plugin_supported_backend_commands() -> &'static [&'static str] {
    &[
        NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS,
        NATIVE_PLUGIN_API_COMMAND_LIST_CONNECTIONS,
        NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION,
        NATIVE_PLUGIN_API_COMMAND_GET_SYSTEM_INFO,
        NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER,
        NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD,
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD,
        NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS,
        NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD,
        NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS,
        NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS,
        NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST,
    ]
}

// api.invoke adapters are the narrow bridge from declared plugin commands to
// native backend services; capability checks stay in the target namespace.
pub struct NativePluginBackendAdapters<'a> {
    pub permissions: &'a plugin_runtime::PluginPermissionSet,
    pub sftp_router: &'a NodeRouter,
    pub sftp_runtime: &'a Arc<tokio::runtime::Runtime>,
    pub forwarding_registry: &'a ForwardingRegistry,
    pub forwarding_runtime: &'a Arc<tokio::runtime::Runtime>,
    pub transfer_manager: &'a Arc<SftpTransferManager>,
}

pub fn native_plugin_api_invoke_response(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    adapters: NativePluginBackendAdapters<'_>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let Some(command) = call.args.get("command").and_then(Value::as_str) else {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "invalid_backend_command",
                "Native plugin api.invoke requires args.command",
            ),
        );
    };
    let declared_commands = native_plugin_declared_api_commands(snapshot, plugin_id);
    if !declared_commands.contains(command) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "backend_command_not_whitelisted",
                format!(
                    "Command \"{command}\" not whitelisted in manifest contributes.apiCommands"
                ),
            ),
        );
    }

    native_plugin_backend_command_response(
        snapshot,
        request_id,
        command,
        call.args.get("args"),
        adapters,
    )
}

fn native_plugin_declared_api_commands(
    snapshot: &NativePluginHostApiSnapshot,
    plugin_id: &str,
) -> HashSet<String> {
    snapshot
        .registry
        .contributions()
        .api_commands
        .iter()
        .filter(|command| command.plugin_id == plugin_id)
        .map(|command| command.command.clone())
        .collect()
}

fn native_plugin_backend_command_response(
    snapshot: &NativePluginHostApiSnapshot,
    request_id: String,
    command: &str,
    args: Option<&Value>,
    adapters: NativePluginBackendAdapters<'_>,
) -> plugin_runtime::PluginResponse {
    let backend_args = args.cloned().unwrap_or_else(|| json!({}));
    match command {
        // Tauri permits plugins to invoke declared commands directly. Native
        // exposes only commands that already have a Workspace-owned adapter so
        // the plugin bridge cannot bypass Rust capability checks.
        NATIVE_PLUGIN_API_COMMAND_SSH_POOL_STATS => {
            plugin_runtime::PluginResponse::ok(request_id, snapshot.pool_stats.clone())
        }
        NATIVE_PLUGIN_API_COMMAND_LIST_CONNECTIONS => {
            plugin_runtime::PluginResponse::ok(request_id, json!(snapshot.connections.clone()))
        }
        NATIVE_PLUGIN_API_COMMAND_GET_APP_VERSION => {
            plugin_runtime::PluginResponse::ok(request_id, json!(env!("CARGO_PKG_VERSION")))
        }
        NATIVE_PLUGIN_API_COMMAND_GET_SYSTEM_INFO => {
            plugin_runtime::PluginResponse::ok(request_id, native_plugin_system_info())
        }
        NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER
        | NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER
        | NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER
        | NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS => native_plugin_transfer_backend_response(
            request_id,
            command,
            &backend_args,
            adapters.transfer_manager,
        ),
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD
        | NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD => native_plugin_sftp_response(
            plugin_runtime::PluginHostCall {
                request_id,
                namespace: "sftp".to_string(),
                method: native_plugin_sftp_backend_method(command).to_string(),
                args: backend_args,
            },
            adapters.permissions,
            adapters.sftp_router,
            adapters.sftp_runtime,
            Some(adapters.transfer_manager),
        ),
        NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS
        | NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD
        | NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS
        | NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS => native_plugin_forward_response(
            plugin_runtime::PluginHostCall {
                request_id,
                namespace: "forward".to_string(),
                method: native_plugin_forward_backend_method(command).to_string(),
                args: backend_args,
            },
            adapters.permissions,
            adapters.forwarding_registry,
            adapters.forwarding_runtime,
            &snapshot.node_connection_ids.values().cloned().collect(),
        ),
        NATIVE_PLUGIN_API_COMMAND_PLUGIN_HTTP_REQUEST => {
            native_plugin_http_request_response(request_id, &backend_args, adapters.sftp_runtime)
        }
        _ => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "backend_command_not_supported",
                format!("Native plugin backend command \"{command}\" is not exposed"),
            ),
        ),
    }
}

fn native_plugin_system_info() -> Value {
    // Tauri exposes this as a lightweight host snapshot. Native keeps the
    // values synchronous so api.invoke cannot start an unowned background task.
    json!({
        "platform": native_plugin_platform_label(),
        "arch": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "family": std::env::consts::FAMILY,
    })
}

fn native_plugin_transfer_backend_response(
    request_id: String,
    command: &str,
    args: &Value,
    manager: &Arc<SftpTransferManager>,
) -> plugin_runtime::PluginResponse {
    let transfer_id = || {
        args.get("transferId")
            .or_else(|| args.get("transfer_id"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("{command} requires args.transferId"))
    };
    match command {
        NATIVE_PLUGIN_API_COMMAND_SFTP_CANCEL_TRANSFER => match transfer_id() {
            Ok(transfer_id) => {
                plugin_runtime::PluginResponse::ok(request_id, json!(manager.cancel(&transfer_id)))
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_transfer_id", error),
            ),
        },
        NATIVE_PLUGIN_API_COMMAND_SFTP_PAUSE_TRANSFER => match transfer_id() {
            Ok(transfer_id) => {
                plugin_runtime::PluginResponse::ok(request_id, json!(manager.pause(&transfer_id)))
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_transfer_id", error),
            ),
        },
        NATIVE_PLUGIN_API_COMMAND_SFTP_RESUME_TRANSFER => match transfer_id() {
            Ok(transfer_id) => {
                plugin_runtime::PluginResponse::ok(request_id, json!(manager.resume(&transfer_id)))
            }
            Err(error) => plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_transfer_id", error),
            ),
        },
        NATIVE_PLUGIN_API_COMMAND_SFTP_TRANSFER_STATS => {
            let stats = manager.transfer_stats();
            plugin_runtime::PluginResponse::ok(
                request_id,
                json!({
                    "active": stats.active,
                    "queued": stats.queued,
                    "completed": stats.completed,
                }),
            )
        }
        _ => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "backend_command_not_supported",
                format!("Native plugin backend command \"{command}\" is not exposed"),
            ),
        ),
    }
}

fn native_plugin_http_request_response(
    request_id: String,
    args: &Value,
    runtime: &Arc<tokio::runtime::Runtime>,
) -> plugin_runtime::PluginResponse {
    let args = args.clone();
    let (response_tx, response_rx) = mpsc::channel();
    // The plugin host-call worker is synchronous. Run the actual HTTP request
    // on the long-lived async runtime so timeouts and socket cleanup are owned
    // by the backend, matching Tauri's command boundary.
    runtime.spawn(async move {
        let result = native_plugin_http_request_result(&args).await;
        let _ = response_tx.send(result);
    });

    match response_rx.recv() {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_http_request_error", error),
        ),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_http_request_unavailable",
                "Native plugin HTTP worker closed before returning a response",
            ),
        ),
    }
}

async fn native_plugin_http_request_result(args: &Value) -> Result<Value, String> {
    let url = args
        .get("url")
        .and_then(Value::as_str)
        .filter(|url| !url.is_empty())
        .ok_or_else(|| "plugin_http_request requires args.url".to_string())?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err("Only HTTP and HTTPS URLs are supported".to_string());
    }
    let method = args
        .get("method")
        .and_then(Value::as_str)
        .filter(|method| !method.is_empty())
        .ok_or_else(|| "plugin_http_request requires args.method".to_string())?;
    let headers = native_plugin_http_headers_arg(args)?;
    let body = match args.get("bodyBase64").or_else(|| args.get("body_base64")) {
        Some(Value::String(encoded)) if !encoded.is_empty() => {
            let bytes = STANDARD
                .decode(encoded)
                .map_err(|error| format!("Invalid base64 request body: {error}"))?;
            if bytes.len() > NATIVE_PLUGIN_HTTP_BODY_LIMIT {
                return Err(format!(
                    "Request body too large: {} bytes (max {} bytes)",
                    bytes.len(),
                    NATIVE_PLUGIN_HTTP_BODY_LIMIT
                ));
            }
            Some(bytes)
        }
        _ => None,
    };

    let client = oxideterm_network_proxy::application_http_client()
        .map_err(|error| format!("Failed to create plugin HTTP client: {error}"))?;
    let mut builder = native_plugin_http_request_builder(&client, url, method, &headers, body)?;
    if let Some(timeout_ms) = args
        .get("timeoutMs")
        .or_else(|| args.get("timeout_ms"))
        .and_then(Value::as_u64)
        .filter(|timeout_ms| *timeout_ms > 0)
    {
        builder = builder.timeout(Duration::from_millis(timeout_ms));
    }

    let response = builder
        .send()
        .await
        .map_err(|error| format!("HTTP request failed: {error}"))?;
    let status = response.status().as_u16();
    let response_headers = response
        .headers()
        .iter()
        .map(|(key, value)| {
            (
                key.to_string(),
                value.to_str().unwrap_or_default().to_string(),
            )
        })
        .collect::<HashMap<_, _>>();
    let bytes = response
        .bytes()
        .await
        .map_err(|error| format!("Failed to read response body: {error}"))?;
    if bytes.len() > NATIVE_PLUGIN_HTTP_BODY_LIMIT {
        return Err(format!(
            "Response too large: {} bytes (max {} bytes)",
            bytes.len(),
            NATIVE_PLUGIN_HTTP_BODY_LIMIT
        ));
    }
    Ok(json!({
        "status": status,
        "headers": response_headers,
        "bodyBase64": STANDARD.encode(bytes),
    }))
}

fn native_plugin_http_headers_arg(args: &Value) -> Result<HashMap<String, String>, String> {
    let Some(headers) = args.get("headers") else {
        return Ok(HashMap::new());
    };
    if headers.is_null() {
        return Ok(HashMap::new());
    }
    serde_json::from_value(headers.clone())
        .map_err(|error| format!("plugin_http_request args.headers must be a string map: {error}"))
}

fn native_plugin_http_request_builder(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    headers: &HashMap<String, String>,
    body: Option<Vec<u8>>,
) -> Result<reqwest::RequestBuilder, String> {
    let method = match method.to_uppercase().as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "DELETE" => reqwest::Method::DELETE,
        "PATCH" => reqwest::Method::PATCH,
        "HEAD" => reqwest::Method::HEAD,
        "MKCOL" => reqwest::Method::from_bytes(b"MKCOL").map_err(|error| error.to_string())?,
        "PROPFIND" => {
            reqwest::Method::from_bytes(b"PROPFIND").map_err(|error| error.to_string())?
        }
        other => return Err(format!("Unsupported HTTP method: {other}")),
    };
    let mut builder = client.request(method, url);
    for (key, value) in headers {
        builder = builder.header(key.as_str(), value.as_str());
    }
    if let Some(body) = body {
        builder = builder.body(body);
    }
    Ok(builder)
}

fn native_plugin_sftp_backend_method(command: &str) -> &'static str {
    match command {
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_INIT => "init",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_LIST_DIR => "listDir",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_STAT => "stat",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_PREVIEW => "preview",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_WRITE => "write",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD => "download",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD => "upload",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_MKDIR => "mkdir",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE => "delete",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DELETE_RECURSIVE => "deleteRecursive",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_RENAME => "rename",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_DOWNLOAD_DIR => "downloadDir",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_UPLOAD_DIR => "uploadDir",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_PROBE => "tarProbe",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_UPLOAD => "tarUpload",
        NATIVE_PLUGIN_API_COMMAND_NODE_SFTP_TAR_DOWNLOAD => "tarDownload",
        _ => "unsupported",
    }
}

fn native_plugin_forward_backend_method(command: &str) -> &'static str {
    match command {
        NATIVE_PLUGIN_API_COMMAND_LIST_PORT_FORWARDS => "list",
        NATIVE_PLUGIN_API_COMMAND_CREATE_PORT_FORWARD => "create",
        NATIVE_PLUGIN_API_COMMAND_STOP_PORT_FORWARD => "stop",
        NATIVE_PLUGIN_API_COMMAND_DELETE_PORT_FORWARD => "delete",
        NATIVE_PLUGIN_API_COMMAND_RESTART_PORT_FORWARD => "restart",
        NATIVE_PLUGIN_API_COMMAND_UPDATE_PORT_FORWARD => "update",
        NATIVE_PLUGIN_API_COMMAND_GET_PORT_FORWARD_STATS => "getStats",
        NATIVE_PLUGIN_API_COMMAND_STOP_ALL_FORWARDS => "stopAll",
        _ => "unsupported",
    }
}
