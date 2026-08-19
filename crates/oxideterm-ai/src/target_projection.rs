use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

/// A runtime-neutral AI target projection without sampled terminal content.
#[derive(Clone, Debug, PartialEq)]
pub struct AiTargetProjection {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub state: String,
    pub capabilities: Vec<String>,
    pub refs: BTreeMap<String, String>,
    pub metadata: Value,
}

/// Non-sensitive node identity needed to expose an SFTP target to AI tools.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiSftpTargetInput {
    pub node_id: String,
    pub session_id: String,
    pub connection_id: Option<String>,
    pub host: String,
}

/// Non-sensitive editor state needed to expose an IDE workspace to AI tools.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiIdeTargetInput {
    pub node_id: String,
    pub connection_id: Option<String>,
    pub active_editor_tab_id: Option<String>,
    pub project_root_path: Option<String>,
    pub project_name: Option<String>,
}

pub fn connect_result_terminal_projection(
    target: &AiTargetProjection,
    original_label: &str,
    node_id: Option<&str>,
    connection_id: Option<&str>,
) -> AiTargetProjection {
    let mut refs = BTreeMap::new();
    insert_optional_ref(&mut refs, "nodeId", node_id);
    if let Some(session_id) = target.refs.get("sessionId") {
        refs.insert("sessionId".to_string(), session_id.clone());
    }
    insert_optional_ref(&mut refs, "connectionId", connection_id);

    AiTargetProjection {
        id: target.id.clone(),
        kind: target.kind.clone(),
        label: format!("{original_label} terminal"),
        state: target.state.clone(),
        capabilities: target.capabilities.clone(),
        refs,
        metadata: json!({ "terminalType": "terminal" }),
    }
}

pub fn opened_local_terminal_projection(target: &AiTargetProjection) -> AiTargetProjection {
    let mut refs = BTreeMap::new();
    if let Some(session_id) = target.refs.get("sessionId") {
        refs.insert("sessionId".to_string(), session_id.clone());
    }

    AiTargetProjection {
        id: target.id.clone(),
        kind: target.kind.clone(),
        label: target.label.clone(),
        state: target.state.clone(),
        capabilities: target.capabilities.clone(),
        refs,
        metadata: json!({ "terminalType": "local_terminal" }),
    }
}

pub fn sftp_target_projection(input: AiSftpTargetInput) -> AiTargetProjection {
    let mut refs = BTreeMap::new();
    refs.insert("nodeId".to_string(), input.node_id);
    refs.insert("sessionId".to_string(), input.session_id.clone());
    if let Some(connection_id) = input.connection_id {
        refs.insert("connectionId".to_string(), connection_id);
    }

    AiTargetProjection {
        id: format!("sftp-session:{}", input.session_id),
        kind: "sftp-session".to_string(),
        label: format!("SFTP {}", input.host),
        state: "connected".to_string(),
        capabilities: string_list(&["filesystem.read", "filesystem.write", "state.list"]),
        refs,
        metadata: json!({ "host": input.host }),
    }
}

pub fn ide_workspace_target_projection(input: AiIdeTargetInput) -> AiTargetProjection {
    let mut refs = BTreeMap::new();
    refs.insert("nodeId".to_string(), input.node_id.clone());
    if let Some(tab_id) = input.active_editor_tab_id.as_ref() {
        refs.insert("tabId".to_string(), tab_id.clone());
    }
    if let Some(connection_id) = input.connection_id {
        refs.insert("connectionId".to_string(), connection_id);
    }

    let mut metadata = Map::new();
    if let Some(root_path) = input.project_root_path {
        metadata.insert("rootPath".to_string(), Value::String(root_path));
    }
    metadata.insert(
        "activeTabId".to_string(),
        input
            .active_editor_tab_id
            .map(Value::String)
            .unwrap_or(Value::Null),
    );

    AiTargetProjection {
        id: format!("ide-workspace:{}", input.node_id),
        kind: "ide-workspace".to_string(),
        label: input
            .project_name
            .unwrap_or_else(|| "IDE workspace".to_string()),
        state: "connected".to_string(),
        capabilities: string_list(&[
            "filesystem.read",
            "filesystem.write",
            "navigation.open",
            "state.list",
        ]),
        refs,
        metadata: Value::Object(metadata),
    }
}

fn insert_optional_ref(refs: &mut BTreeMap<String, String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        refs.insert(key.to_string(), value.to_string());
    }
}

fn string_list(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sftp_projection_uses_only_non_sensitive_node_identity() {
        let projection = sftp_target_projection(AiSftpTargetInput {
            node_id: "node-a".to_string(),
            session_id: "sftp-a".to_string(),
            connection_id: Some("connection-a".to_string()),
            host: "example.internal".to_string(),
        });

        assert_eq!(projection.id, "sftp-session:sftp-a");
        assert_eq!(projection.label, "SFTP example.internal");
        assert_eq!(projection.metadata, json!({ "host": "example.internal" }));
    }
}
