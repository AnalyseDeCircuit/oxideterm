// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! IDE host API responses and plugin-facing snapshot projections.

use std::collections::HashMap;

use oxideterm_gpui_ide::{IdePluginFileSnapshot, IdePluginSnapshot};
use oxideterm_plugin_protocol as plugin_runtime;
use serde_json::{Value, json};

pub fn native_plugin_ide_response(
    call: plugin_runtime::PluginHostCall,
    snapshot: &Value,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match call.method.as_str() {
        "isOpen" => plugin_runtime::PluginResponse::ok(
            request_id,
            json!(
                snapshot
                    .get("isOpen")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            ),
        ),
        "getProject" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot.get("project").cloned().unwrap_or(Value::Null),
        ),
        "getOpenFiles" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot
                .get("openFiles")
                .cloned()
                .unwrap_or_else(|| json!([])),
        ),
        "getActiveFile" => plugin_runtime::PluginResponse::ok(
            request_id,
            snapshot.get("activeFile").cloned().unwrap_or(Value::Null),
        ),
        "onFileOpen" | "onFileClose" | "onActiveFileChange" => {
            plugin_runtime::PluginResponse::error(
                request_id,
                plugin_runtime::PluginError::runtime(
                    "plugin_ide_subscription_bridge",
                    "IDE subscriptions are registered through the runtime event bridge",
                ),
            )
        }
        method => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::protocol(
                "unknown_ide_method",
                format!("Unknown ide.{method} host API"),
            ),
        ),
    }
}

pub fn native_plugin_ide_snapshot_value(snapshot: &IdePluginSnapshot) -> Value {
    // This projection mirrors Tauri's ideStore snapshot without exposing file
    // content, tree nodes, agent process state, or reconnect-only metadata.
    json!({
        "isOpen": true,
        "project": {
            "nodeId": &snapshot.project.node_id,
            "rootPath": &snapshot.project.root_path,
            "name": &snapshot.project.name,
            "isGitRepo": snapshot.project.is_git_repo,
            "gitBranch": &snapshot.project.git_branch,
        },
        "openFiles": snapshot
            .open_files
            .iter()
            .map(native_plugin_ide_file_snapshot)
            .collect::<Vec<_>>(),
        "activeFile": snapshot
            .active_file
            .as_ref()
            .map(native_plugin_ide_file_snapshot),
    })
}

fn native_plugin_ide_file_snapshot(file: &IdePluginFileSnapshot) -> Value {
    json!({
        "path": &file.path,
        "name": &file.name,
        "language": &file.language,
        "isDirty": file.is_dirty,
        "isActive": file.is_active,
        "isPinned": file.is_pinned,
    })
}

pub fn native_plugin_ide_file_map(snapshot: &Value) -> HashMap<String, Value> {
    snapshot
        .get("openFiles")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|file| {
            let path = file.get("path").and_then(Value::as_str)?;
            Some((path.to_string(), file.clone()))
        })
        .collect()
}

pub fn native_plugin_ide_active_file_path(snapshot: &Value) -> Option<String> {
    snapshot
        .get("activeFile")
        .and_then(|file| file.get("path"))
        .and_then(Value::as_str)
        .map(str::to_string)
}
