// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! IDE host API responses and plugin-facing snapshot projections.

use std::collections::HashMap;

use oxideterm_ide_core::{IdePluginFileSnapshot, IdePluginSnapshot};
use oxideterm_plugin_protocol as plugin_runtime;
use serde_json::{Value, json};

use crate::app::native_plugin_ide_state_summary;

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
        "getSummary" => plugin_runtime::PluginResponse::ok(
            request_id,
            native_plugin_ide_state_summary(snapshot),
        ),
        "openFile" | "closeFile" => {
            if call
                .args
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|path| !path.trim().is_empty())
            {
                plugin_runtime::PluginResponse::ok(request_id, json!({ "queued": true }))
            } else {
                plugin_runtime::PluginResponse::error(
                    request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_ide_path",
                        format!("ide.{} requires non-empty args.path", call.method),
                    ),
                )
            }
        }
        "replaceActiveText" | "insertActiveText" => {
            if call.args.get("text").and_then(Value::as_str).is_some() {
                plugin_runtime::PluginResponse::ok(request_id, json!({ "queued": true }))
            } else {
                plugin_runtime::PluginResponse::error(
                    request_id,
                    plugin_runtime::PluginError::protocol(
                        "invalid_ide_text",
                        format!("ide.{} requires string args.text", call.method),
                    ),
                )
            }
        }
        "saveActive" | "refreshProject" => {
            plugin_runtime::PluginResponse::ok(request_id, json!({ "queued": true }))
        }
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

#[cfg(test)]
mod tests {
    use oxideterm_ide_core::{IdePluginFileSnapshot, IdePluginProjectSnapshot, IdePluginSnapshot};
    use serde_json::json;

    use super::{native_plugin_ide_response, native_plugin_ide_snapshot_value};

    #[test]
    fn snapshot_value_projects_core_dto_without_ui_state() {
        // This test keeps the plugin boundary tied to the UI-independent IDE DTOs.
        let snapshot = IdePluginSnapshot {
            project: IdePluginProjectSnapshot {
                node_id: "node-1".to_string(),
                root_path: "/srv/project".to_string(),
                name: "project".to_string(),
                is_git_repo: true,
                git_branch: Some("main".to_string()),
            },
            open_files: vec![IdePluginFileSnapshot {
                path: "/srv/project/src/main.rs".to_string(),
                name: "main.rs".to_string(),
                language: "Rust".to_string(),
                is_dirty: true,
                is_active: true,
                is_pinned: false,
            }],
            active_file: None,
        };

        assert_eq!(
            native_plugin_ide_snapshot_value(&snapshot),
            json!({
                "isOpen": true,
                "project": {
                    "nodeId": "node-1",
                    "rootPath": "/srv/project",
                    "name": "project",
                    "isGitRepo": true,
                    "gitBranch": "main",
                },
                "openFiles": [{
                    "path": "/srv/project/src/main.rs",
                    "name": "main.rs",
                    "language": "Rust",
                    "isDirty": true,
                    "isActive": true,
                    "isPinned": false,
                }],
                "activeFile": null,
            })
        );
    }

    #[test]
    fn summary_response_exposes_editor_state_without_names_or_paths() {
        let snapshot = json!({
            "isOpen": true,
            "project": {
                "nodeId": "node-1",
                "rootPath": "/private/project",
                "name": "private-project",
                "isGitRepo": true,
                "gitBranch": "secret-feature",
            },
            "openFiles": [{
                "path": "/private/project/src/main.rs",
                "name": "main.rs",
                "language": "Rust",
                "isDirty": true,
                "isPinned": true,
                "content": "private source",
            }],
            "activeFile": {
                "path": "/private/project/src/main.rs",
                "name": "main.rs",
                "language": "Rust",
            },
        });
        let response = native_plugin_ide_response(
            oxideterm_plugin_protocol::PluginHostCall {
                request_id: "ide.getSummary".to_string(),
                namespace: "ide".to_string(),
                method: "getSummary".to_string(),
                args: serde_json::Value::Null,
            },
            &snapshot,
        );
        let oxideterm_plugin_protocol::PluginResponseResult::Ok { value } = response.result else {
            panic!("IDE summary should return safe metadata");
        };

        assert_eq!(value["openFileCount"], json!(1));
        assert_eq!(value["dirtyFileCount"], json!(1));
        assert_eq!(value["activeFileLanguage"], json!("Rust"));
        let serialized = value.to_string();
        assert!(!serialized.contains("/private/project"));
        assert!(!serialized.contains("private-project"));
        assert!(!serialized.contains("secret-feature"));
        assert!(!serialized.contains("private source"));
    }
}
