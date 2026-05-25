// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    future::Future,
    path::Path,
    pin::Pin,
    sync::{Arc, mpsc},
};

use oxideterm_sftp::{
    ListFilter, PreviewContent, SftpError, SftpSession, SftpTransferManager, encode_to_encoding,
    probe_tar_support, tar_download_directory, tar_upload_directory,
};
use oxideterm_ssh::{NodeId, NodeRouter};
use serde_json::{Value, json};

use oxideterm_plugin_protocol as plugin_runtime;

use crate::capabilities::{
    NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ, NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE,
};

type NativePluginSharedSftp = Arc<tokio::sync::Mutex<SftpSession>>;
type NativePluginSftpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, SftpError>> + Send + 'a>>;

// SFTP calls cross from the synchronous plugin host-call hook into the
// NodeRouter-owned async runtime, so the bridge and argument validation live here.
pub fn native_plugin_sftp_response(
    call: plugin_runtime::PluginHostCall,
    permissions: &plugin_runtime::PluginPermissionSet,
    router: &NodeRouter,
    runtime: &Arc<tokio::runtime::Runtime>,
    transfer_manager: Option<&Arc<SftpTransferManager>>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    if let Err(error) = native_plugin_sftp_check_capability(&call.method, permissions) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("plugin_sftp_capability_denied", error),
        );
    }
    let method = call.method.clone();
    let args = call.args.clone();
    let router = router.clone();
    let transfer_manager = transfer_manager.cloned();
    let (response_tx, response_rx) = mpsc::channel();

    // The stdio host-call hook is synchronous, while SFTP is owned by the
    // NodeRouter async runtime. Spawn the real protocol operation on that
    // backend runtime and block only this plugin host-call worker until it
    // finishes, preserving Tauri's Promise-returning ctx.sftp shape.
    runtime.spawn(async move {
        let result = native_plugin_sftp_result(&router, &method, &args, transfer_manager).await;
        let _ = response_tx.send(result);
    });

    match response_rx.recv() {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_sftp_error", error),
        ),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_sftp_unavailable",
                "Native plugin SFTP worker closed before returning a response",
            ),
        ),
    }
}

pub fn native_plugin_sftp_check_capability(
    method: &str,
    permissions: &plugin_runtime::PluginPermissionSet,
) -> Result<(), String> {
    let required = match method {
        "init" | "listDir" | "stat" | "readFile" | "preview" | "download" | "downloadDir"
        | "tarProbe" | "tarDownload" => NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ,
        "writeFile" | "write" | "upload" | "mkdir" | "delete" | "deleteRecursive" | "rename"
        | "uploadDir" | "tarUpload" => NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE,
        _ => return Ok(()),
    };
    if permissions
        .capabilities
        .iter()
        .any(|capability| capability == required)
    {
        return Ok(());
    }
    Err(format!(
        "Native plugin SFTP host call \"{method}\" requires capability \"{required}\""
    ))
}

async fn native_plugin_sftp_result(
    router: &NodeRouter,
    method: &str,
    args: &Value,
    transfer_manager: Option<Arc<SftpTransferManager>>,
) -> Result<Value, String> {
    match method {
        "init" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let cwd = native_plugin_with_sftp(router, &node_id, |sftp| {
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    Ok(sftp.cwd().to_string())
                })
            })
            .await?;
            Ok(json!(cwd))
        }
        "listDir" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let filter = native_plugin_sftp_list_filter_arg(args)?;
            let entries = native_plugin_with_sftp_retry(router, &node_id, |sftp| {
                let path = path.clone();
                let filter = filter.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.list_dir(&path, filter).await
                })
            })
            .await?;
            Ok(json!(entries))
        }
        "stat" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let info = native_plugin_with_sftp_retry(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.stat(&path).await
                })
            })
            .await?;
            Ok(json!(info))
        }
        "readFile" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let preview = native_plugin_with_sftp_retry(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.preview(&path).await
                })
            })
            .await?;
            match preview {
                PreviewContent::Text { data, .. } => Ok(json!(data)),
                _ => Err("File is not a text file or exceeds size limit".to_string()),
            }
        }
        "preview" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let preview = native_plugin_with_sftp_retry(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.preview(&path).await
                })
            })
            .await?;
            Ok(json!(preview))
        }
        "writeFile" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let content = native_plugin_sftp_content_arg(args)?;
            native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                let content = content.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.write_content(&path, content.as_bytes())
                        .await
                        .map(|_| ())
                })
            })
            .await?;
            Ok(Value::Null)
        }
        "write" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let content = native_plugin_sftp_content_arg(args)?;
            let encoding = args
                .get("encoding")
                .and_then(Value::as_str)
                .filter(|encoding| !encoding.is_empty())
                .unwrap_or("UTF-8")
                .to_string();
            let encoded_content = encode_to_encoding(&content, &encoding);
            let file_info = native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                let encoded_content = encoded_content.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.write_content(&path, &encoded_content).await?;
                    sftp.stat(&path).await
                })
            })
            .await?;
            Ok(json!({
                "mtime": (file_info.modified > 0).then_some(file_info.modified as u64),
                "size": Some(file_info.size),
                "encodingUsed": encoding,
                "atomicWrite": false,
            }))
        }
        "download" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let byte_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let remote_path = remote_path.clone();
                let local_path = local_path.clone();
                let transfer_id = transfer_id.clone();
                let transfer_manager = transfer_manager.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.download_file(
                        &remote_path,
                        &local_path,
                        &transfer_id,
                        None,
                        transfer_manager,
                    )
                    .await
                })
            })
            .await?;
            Ok(json!(byte_count))
        }
        "upload" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let byte_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let local_path = local_path.clone();
                let remote_path = remote_path.clone();
                let transfer_id = transfer_id.clone();
                let transfer_manager = transfer_manager.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.upload_file(
                        &local_path,
                        &remote_path,
                        &transfer_id,
                        None,
                        transfer_manager,
                    )
                    .await
                })
            })
            .await?;
            Ok(json!(byte_count))
        }
        "mkdir" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.mkdir(&path).await
                })
            })
            .await?;
            Ok(Value::Null)
        }
        "delete" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.delete(&path).await
                })
            })
            .await?;
            Ok(Value::Null)
        }
        "deleteRecursive" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let path = native_plugin_sftp_path_arg(args, "path")?;
            let deleted_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let path = path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.delete_recursive(&path).await
                })
            })
            .await?;
            Ok(json!(deleted_count))
        }
        "downloadDir" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let item_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let remote_path = remote_path.clone();
                let local_path = local_path.clone();
                let transfer_id = transfer_id.clone();
                let transfer_manager = transfer_manager.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.download_dir(
                        &remote_path,
                        &local_path,
                        &transfer_id,
                        None,
                        transfer_manager,
                    )
                    .await
                })
            })
            .await?;
            Ok(json!(item_count))
        }
        "uploadDir" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let item_count = native_plugin_with_sftp(router, &node_id, |sftp| {
                let local_path = local_path.clone();
                let remote_path = remote_path.clone();
                let transfer_id = transfer_id.clone();
                let transfer_manager = transfer_manager.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.upload_dir(
                        &local_path,
                        &remote_path,
                        &transfer_id,
                        None,
                        transfer_manager,
                    )
                    .await
                })
            })
            .await?;
            Ok(json!(item_count))
        }
        "tarProbe" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let resolved = router
                .resolve_connection(&node_id)
                .await
                .map_err(native_plugin_route_error)?;
            Ok(json!(probe_tar_support(&resolved.handle).await))
        }
        "tarUpload" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let resolved = router
                .resolve_connection(&node_id)
                .await
                .map_err(native_plugin_route_error)?;
            let item_count = tar_upload_directory(
                &resolved.handle,
                &local_path,
                &remote_path,
                &transfer_id,
                None,
                transfer_manager,
                None,
            )
            .await
            .map_err(native_plugin_sftp_error)?;
            Ok(json!(item_count))
        }
        "tarDownload" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let remote_path = native_plugin_sftp_path_arg(args, "remotePath")?;
            let local_path = native_plugin_sftp_local_path_arg(args, "localPath")?;
            let transfer_id = native_plugin_sftp_transfer_id_arg(args);
            let resolved = router
                .resolve_connection(&node_id)
                .await
                .map_err(native_plugin_route_error)?;
            let item_count = tar_download_directory(
                &resolved.handle,
                &remote_path,
                &local_path,
                &transfer_id,
                None,
                transfer_manager,
                None,
            )
            .await
            .map_err(native_plugin_sftp_error)?;
            Ok(json!(item_count))
        }
        "rename" => {
            let node_id = native_plugin_sftp_node_id_arg(args)?;
            let old_path = native_plugin_sftp_path_arg(args, "oldPath")?;
            let new_path = native_plugin_sftp_path_arg(args, "newPath")?;
            native_plugin_with_sftp(router, &node_id, |sftp| {
                let old_path = old_path.clone();
                let new_path = new_path.clone();
                Box::pin(async move {
                    let sftp = sftp.lock().await;
                    sftp.rename(&old_path, &new_path).await
                })
            })
            .await?;
            Ok(Value::Null)
        }
        method => Err(format!("Unsupported SFTP host call: {method}")),
    }
}

async fn native_plugin_with_sftp<T, F>(
    router: &NodeRouter,
    node_id: &NodeId,
    operation: F,
) -> Result<T, String>
where
    F: for<'a> Fn(&'a NativePluginSharedSftp) -> NativePluginSftpFuture<'a, T>,
{
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(native_plugin_route_error)?;
    operation(&sftp).await.map_err(native_plugin_sftp_error)
}

async fn native_plugin_with_sftp_retry<T, F>(
    router: &NodeRouter,
    node_id: &NodeId,
    operation: F,
) -> Result<T, String>
where
    F: for<'a> Fn(&'a NativePluginSharedSftp) -> NativePluginSftpFuture<'a, T>,
{
    let sftp = router
        .acquire_sftp(node_id)
        .await
        .map_err(native_plugin_route_error)?;
    match operation(&sftp).await {
        Ok(value) => Ok(value),
        Err(error) if error.is_channel_recoverable() => {
            // Mirrors Tauri's read-only sftp_with_retry! behavior: stale
            // channels are invalidated at the NodeRouter owner and retried once
            // without tying SFTP lifetime to any terminal pane.
            let sftp = router
                .invalidate_and_reacquire_sftp(node_id)
                .await
                .map_err(native_plugin_route_error)?;
            operation(&sftp).await.map_err(native_plugin_sftp_error)
        }
        Err(error) => Err(native_plugin_sftp_error(error)),
    }
}

pub fn native_plugin_sftp_node_id_arg(args: &Value) -> Result<NodeId, String> {
    let node_id = args
        .get("nodeId")
        .and_then(Value::as_str)
        .filter(|node_id| !node_id.is_empty())
        .ok_or_else(|| "sftp host call requires args.nodeId".to_string())?;
    Ok(NodeId::new(node_id.to_string()))
}

pub fn native_plugin_sftp_path_arg(args: &Value, field: &str) -> Result<String, String> {
    let path = args
        .get(field)
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .ok_or_else(|| format!("sftp host call requires args.{field}"))?;
    if path.contains('\0') {
        return Err(format!("sftp args.{field} contains an invalid NUL byte"));
    }
    Ok(path.to_string())
}

fn native_plugin_sftp_local_path_arg(args: &Value, field: &str) -> Result<String, String> {
    let path = native_plugin_sftp_path_arg(args, field)?;
    if Path::new(&path).is_absolute() {
        return Ok(path);
    }
    Err(format!("sftp args.{field} must be an absolute local path"))
}

fn native_plugin_sftp_transfer_id_arg(args: &Value) -> String {
    args.get("transferId")
        .or_else(|| args.get("transfer_id"))
        .and_then(Value::as_str)
        .filter(|transfer_id| !transfer_id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

fn native_plugin_sftp_list_filter_arg(args: &Value) -> Result<Option<ListFilter>, String> {
    let Some(filter) = args.get("filter") else {
        return Ok(None);
    };
    if filter.is_null() {
        return Ok(None);
    }
    serde_json::from_value(filter.clone())
        .map(Some)
        .map_err(|error| {
            format!("sftp list_dir args.filter does not match the native ListFilter shape: {error}")
        })
}

fn native_plugin_sftp_content_arg(args: &Value) -> Result<String, String> {
    args.get("content")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "sftp.writeFile requires args.content".to_string())
}

fn native_plugin_sftp_error(error: SftpError) -> String {
    error.to_string()
}

fn native_plugin_route_error(error: oxideterm_ssh::RouteError) -> String {
    error.to_string()
}
