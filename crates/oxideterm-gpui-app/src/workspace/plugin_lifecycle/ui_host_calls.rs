// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::mpsc;

use serde_json::{Value, json};

use super::{
    sync::native_plugin_emit_sync_progress,
    types::{NativePluginConfirmRequest, NativePluginSyncRequest},
};
use crate::workspace::plugin_runtime;

// These UI host calls bounce synchronous plugin requests into Workspace-owned
// UI channels while preserving the JS Promise-style response shape.
pub(super) fn native_plugin_show_confirm_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    confirm_tx: &mpsc::Sender<NativePluginConfirmRequest>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let (title, description) = match native_plugin_confirm_args(&call.args) {
        Ok(args) => args,
        Err(error) => {
            return plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::protocol("invalid_confirm_args", error),
            );
        }
    };
    let (response_tx, response_rx) = mpsc::channel();
    let request = NativePluginConfirmRequest {
        plugin_id: plugin_id.to_string(),
        request_id: request_id.clone(),
        title,
        description,
        response_tx,
    };
    if confirm_tx.send(request).is_err() {
        return plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "confirm_host_unavailable",
                "Native plugin ui.showConfirm cannot reach the workspace dialog host",
            ),
        );
    }

    // Match Tauri's Promise<boolean> semantics: the plugin request waits for
    // the user's protected native dialog choice instead of receiving a default.
    match response_rx.recv() {
        Ok(confirmed) => plugin_runtime::PluginResponse::ok(request_id, json!(confirmed)),
        Err(_) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime(
                "confirm_response_unavailable",
                "Native plugin ui.showConfirm closed before the workspace answered",
            ),
        ),
    }
}

fn native_plugin_confirm_args(args: &Value) -> Result<(String, String), String> {
    let title = args
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .ok_or_else(|| "ui.showConfirm requires args.title".to_string())?;
    let description = args
        .get("description")
        .and_then(Value::as_str)
        .ok_or_else(|| "ui.showConfirm requires args.description".to_string())?;
    Ok((title.to_string(), description.to_string()))
}

pub(super) fn native_plugin_show_progress_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    sync_tx: Option<&mpsc::Sender<NativePluginSyncRequest>>,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    let title = call
        .args
        .get("title")
        .and_then(Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("Plugin progress");
    let registration_id = call
        .args
        .get("registrationId")
        .or_else(|| call.args.get("id"))
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    native_plugin_emit_sync_progress(
        sync_tx,
        plugin_id,
        &registration_id,
        json!({
            "title": title,
            "message": call.args.get("message").and_then(Value::as_str),
            "progress": 0.0,
            "done": false,
        }),
    );

    plugin_runtime::PluginResponse::ok(
        request_id,
        json!({
            "id": registration_id,
            "registrationId": registration_id,
        }),
    )
}
