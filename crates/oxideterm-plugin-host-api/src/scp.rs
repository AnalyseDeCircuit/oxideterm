// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{path::Path, sync::Arc, sync::mpsc};

use oxideterm_plugin_protocol as plugin_runtime;
use oxideterm_sftp::{
    BackgroundTransferDirection, BackgroundTransferKind, BackgroundTransferSnapshot,
    BackgroundTransferState, SftpTransferGuard, SftpTransferManager, TransferProgress,
    TransferProtocol, TransferStrategy, scp_download_directory, scp_download_file,
    scp_upload_directory, scp_upload_file,
};
use oxideterm_ssh::{NodeId, NodeRouter};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::capabilities::{
    NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ, NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE,
};

/// Dispatches SCP calls on the same runtime and node-owned SSH connection used by SFTP.
pub fn native_plugin_scp_response(
    call: plugin_runtime::PluginHostCall,
    permissions: &plugin_runtime::PluginPermissionSet,
    router: &NodeRouter,
    runtime: &Arc<tokio::runtime::Runtime>,
    transfer_manager: &Arc<SftpTransferManager>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    if let Err(error) = native_plugin_scp_check_capability(&call.method, permissions) {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol("plugin_scp_capability_denied", error),
        );
    }
    let method = call.method;
    let args = call.args;
    let router = router.clone();
    let manager = transfer_manager.clone();
    let (response_tx, response_rx) = mpsc::channel();

    // The plugin bridge is synchronous, while SCP remains owned by the retained SSH runtime.
    runtime.spawn(async move {
        let result = native_plugin_scp_result(&router, &manager, &method, &args).await;
        let _ = response_tx.send(result);
    });

    match response_rx.recv() {
        Ok(Ok(value)) => plugin_runtime::PluginResponse::ok(request_id, value),
        Ok(Err(error)) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_scp_error", error),
        ),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "plugin_scp_unavailable",
                "Native plugin SCP worker closed before returning a response",
            ),
        ),
    }
}

pub fn native_plugin_scp_check_capability(
    method: &str,
    permissions: &plugin_runtime::PluginPermissionSet,
) -> Result<(), String> {
    let required = match method {
        "capabilities" | "download" | "downloadDirectory" => {
            NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ
        }
        "upload" | "uploadDirectory" => NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE,
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
        "Native plugin SCP host call \"{method}\" requires capability \"{required}\""
    ))
}

async fn native_plugin_scp_result(
    router: &NodeRouter,
    manager: &Arc<SftpTransferManager>,
    method: &str,
    args: &Value,
) -> Result<Value, String> {
    let node_id = scp_node_id_arg(args)?;
    let resolved = router
        .resolve_connection(&node_id)
        .await
        .map_err(|error| error.to_string())?;
    if method == "capabilities" {
        let capabilities = manager
            .scp_capabilities(&resolved.connection_id, &resolved.handle)
            .await;
        return Ok(json!({
            "supported": capabilities.supports_scp,
            "recursive": capabilities.supports_recursive,
            "restartResume": false,
        }));
    }

    let transfer_id = args
        .get("transferId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("plugin-scp-{}", Uuid::new_v4()));
    // Register the node owner before entering the protocol pump so explicit
    // node disconnect interrupts plugin-started transfers as well.
    let _control = manager.register_for_node(&transfer_id, node_id.0.clone());
    let _guard = SftpTransferGuard::new(Some(manager), transfer_id.clone());
    let local_path = scp_path_arg(args, "localPath")?;
    let remote_path = scp_path_arg(args, "remotePath")?;
    let (direction, kind) = match method {
        "upload" => (
            BackgroundTransferDirection::Upload,
            BackgroundTransferKind::File,
        ),
        "download" => (
            BackgroundTransferDirection::Download,
            BackgroundTransferKind::File,
        ),
        "uploadDirectory" => (
            BackgroundTransferDirection::Upload,
            BackgroundTransferKind::Directory,
        ),
        "downloadDirectory" => (
            BackgroundTransferDirection::Download,
            BackgroundTransferKind::Directory,
        ),
        _ => return Err(format!("Unsupported SCP host call: {method}")),
    };
    let display_path = match direction {
        BackgroundTransferDirection::Upload => &local_path,
        BackgroundTransferDirection::Download => &remote_path,
    };
    let mut snapshot = BackgroundTransferSnapshot::new(
        transfer_id.clone(),
        node_id.0.clone(),
        Path::new(display_path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(display_path)
            .to_string(),
        local_path.clone(),
        remote_path.clone(),
        direction,
        kind,
        if kind == BackgroundTransferKind::Directory {
            TransferStrategy::DirectoryRecursive
        } else {
            TransferStrategy::File
        },
        0,
        0,
    );
    snapshot.protocol = TransferProtocol::Scp;
    manager.register_background_transfer(snapshot);
    manager.mark_background_transfer_active(&transfer_id);
    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<TransferProgress>(100);
    let progress_manager = manager.clone();
    let progress_transfer_id = transfer_id.clone();
    // Plugin calls are synchronous at the protocol boundary, but their progress
    // remains observable through the shared transfer APIs while bytes move.
    tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            progress_manager.update_background_transfer_progress(
                &progress_transfer_id,
                progress.transferred_bytes,
                progress.total_bytes,
                progress.speed,
            );
        }
    });
    let result = match method {
        "upload" => {
            scp_upload_file(
                &resolved.handle,
                &local_path,
                &remote_path,
                &transfer_id,
                Some(progress_tx),
                Some(manager.clone()),
            )
            .await
        }
        "download" => {
            scp_download_file(
                &resolved.handle,
                &remote_path,
                &local_path,
                &transfer_id,
                Some(progress_tx),
                Some(manager.clone()),
            )
            .await
        }
        "uploadDirectory" => {
            scp_upload_directory(
                &resolved.handle,
                &local_path,
                &remote_path,
                &transfer_id,
                Some(progress_tx),
                Some(manager.clone()),
            )
            .await
        }
        "downloadDirectory" => {
            scp_download_directory(
                &resolved.handle,
                &remote_path,
                &local_path,
                &transfer_id,
                Some(progress_tx),
                Some(manager.clone()),
            )
            .await
        }
        _ => unreachable!("SCP method was validated before transfer registration"),
    };
    match &result {
        Ok(result) => {
            manager.finish_background_transfer(
                &transfer_id,
                BackgroundTransferState::Completed,
                None,
                Some(result.items),
            );
        }
        Err(error) if matches!(error, oxideterm_sftp::SftpError::TransferCancelled) => {
            manager.finish_background_transfer(
                &transfer_id,
                BackgroundTransferState::Cancelled,
                None,
                None,
            );
        }
        Err(error) => {
            manager.finish_background_transfer(
                &transfer_id,
                BackgroundTransferState::Error,
                Some(error.to_string()),
                None,
            );
        }
    }
    let result = result.map_err(|error| error.to_string())?;
    Ok(json!({
        "transferId": transfer_id,
        "protocol": "scp",
        "bytes": result.bytes,
        "items": result.items,
        "restartResume": false,
    }))
}

fn scp_node_id_arg(args: &Value) -> Result<NodeId, String> {
    let node_id = args
        .get("nodeId")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "SCP calls require a non-empty args.nodeId".to_string())?;
    Ok(NodeId::new(node_id))
}

fn scp_path_arg(args: &Value, field: &str) -> Result<String, String> {
    args.get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("SCP calls require a non-empty args.{field}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn permissions(capabilities: &[&str]) -> plugin_runtime::PluginPermissionSet {
        plugin_runtime::PluginPermissionSet {
            capabilities: capabilities
                .iter()
                .map(|capability| capability.to_string())
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn scp_methods_use_existing_filesystem_capabilities() {
        let read = permissions(&[NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_READ]);
        let write = permissions(&[NATIVE_PLUGIN_CAPABILITY_FILESYSTEM_WRITE]);
        assert!(native_plugin_scp_check_capability("download", &read).is_ok());
        assert!(native_plugin_scp_check_capability("upload", &write).is_ok());
        assert!(native_plugin_scp_check_capability("upload", &read).is_err());
        assert!(native_plugin_scp_check_capability("download", &write).is_err());
    }
}
