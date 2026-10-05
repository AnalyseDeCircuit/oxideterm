// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::Deserialize;
use serde_json::Value;

/// Discovery supplies connection metadata; credentials and execution stay in the host form.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscoveredSshHost {
    pub name: String,
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub group: String,
}

pub fn discovered_ssh_host(args: &Value) -> Result<DiscoveredSshHost, &'static str> {
    let host: DiscoveredSshHost =
        serde_json::from_value(args.clone()).map_err(|_| "Invalid discovered SSH host")?;
    if host.host.is_empty()
        || host.host.len() > 253
        || host.host.starts_with('-')
        || !host
            .host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b".-_:%".contains(&byte))
        || host.port == 0
        || host.name.trim().is_empty()
        || [&host.name, &host.username, &host.group]
            .iter()
            .any(|value| value.len() > 256 || value.chars().any(char::is_control))
        || host.username.chars().any(char::is_whitespace)
    {
        return Err("Invalid discovered SSH host");
    }
    Ok(host)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorkspaceDestination {
    Tab {
        id: String,
    },
    Page {
        page: WorkspacePage,
    },
    Sftp {
        #[serde(rename = "nodeId")]
        node_id: String,
    },
    Forwards {
        #[serde(rename = "nodeId")]
        node_id: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspacePage {
    Sessions,
    Files,
    Plugins,
    CloudSync,
    Notifications,
    LocalTerminal,
}

pub fn validate_workspace_destination(
    args: &Value,
    summary: &Value,
    nodes: &std::collections::HashMap<String, String>,
) -> Result<WorkspaceDestination, &'static str> {
    let destination: WorkspaceDestination =
        serde_json::from_value(args.clone()).map_err(|_| "Invalid workspace destination")?;
    match &destination {
        WorkspaceDestination::Tab { id } => {
            if !summary["tabs"]
                .as_array()
                .is_some_and(|tabs| tabs.iter().any(|tab| tab["id"].as_str() == Some(id)))
            {
                return Err("The requested workspace tab is no longer open");
            }
        }
        WorkspaceDestination::Sftp { node_id } | WorkspaceDestination::Forwards { node_id } => {
            if !nodes.contains_key(node_id) {
                return Err("The requested workspace node no longer exists");
            }
        }
        WorkspaceDestination::Page { .. } => {}
    }
    Ok(destination)
}
