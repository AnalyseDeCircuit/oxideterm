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

/// Neutral Raw TCP settings used to describe a local socket target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiRawTcpTargetInput {
    pub endpoint_label: String,
    pub host: String,
    pub port: u16,
    pub line_ending: String,
    pub display_mode: String,
    pub send_mode: String,
    pub tls_enabled: bool,
    pub tls_verification: String,
    pub tls_server_name: Option<String>,
}

/// Neutral Raw UDP settings used to describe a local datagram target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiRawUdpTargetInput {
    pub remote_endpoint_label: String,
    pub remote_host: String,
    pub remote_port: u16,
    pub local_bind_host: Option<String>,
    pub local_bind_port: u16,
    pub line_ending: String,
    pub display_mode: String,
    pub send_mode: String,
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

pub fn raw_tcp_terminal_label(input: &AiRawTcpTargetInput) -> String {
    let scheme = if input.tls_enabled { "TLS" } else { "TCP" };
    format!("{scheme} {}", input.endpoint_label)
}

pub fn raw_tcp_terminal_metadata(input: &AiRawTcpTargetInput) -> Value {
    json!({
        "terminalType": "raw_tcp",
        "terminalTransport": "raw_tcp",
        "host": input.host,
        "port": input.port,
        "lineEnding": input.line_ending,
        "displayMode": input.display_mode,
        "sendMode": input.send_mode,
        "tls": {
            "enabled": input.tls_enabled,
            "verification": input.tls_verification,
            "serverName": input.tls_server_name,
        },
    })
}

pub fn raw_udp_terminal_label(input: &AiRawUdpTargetInput) -> String {
    format!("UDP {}", input.remote_endpoint_label)
}

pub fn raw_udp_terminal_metadata(input: &AiRawUdpTargetInput) -> Value {
    json!({
        "terminalType": "raw_udp",
        "terminalTransport": "raw_udp",
        "remoteHost": input.remote_host,
        "remotePort": input.remote_port,
        "localBindHost": input.local_bind_host,
        "localBindPort": input.local_bind_port,
        "lineEnding": input.line_ending,
        "displayMode": input.display_mode,
        "sendMode": input.send_mode,
    })
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

    fn terminal_projection() -> AiTargetProjection {
        AiTargetProjection {
            id: "terminal-session:7".to_string(),
            kind: "terminal-session".to_string(),
            label: "SSH terminal 7".to_string(),
            state: "connected".to_string(),
            capabilities: string_list(&["terminal.observe", "terminal.send"]),
            refs: BTreeMap::from([
                ("sessionId".to_string(), "7".to_string()),
                ("tabId".to_string(), "9".to_string()),
            ]),
            metadata: json!({ "paneId": 3 }),
        }
    }

    #[test]
    fn connect_result_keeps_only_connection_runtime_refs() {
        let projection = connect_result_terminal_projection(
            &terminal_projection(),
            "Production",
            Some("node-a"),
            Some("connection-a"),
        );

        assert_eq!(projection.label, "Production terminal");
        assert_eq!(
            projection.refs,
            BTreeMap::from([
                ("connectionId".to_string(), "connection-a".to_string()),
                ("nodeId".to_string(), "node-a".to_string()),
                ("sessionId".to_string(), "7".to_string()),
            ])
        );
        assert_eq!(projection.metadata, json!({ "terminalType": "terminal" }));
    }

    #[test]
    fn opened_local_terminal_drops_tab_metadata() {
        let projection = opened_local_terminal_projection(&terminal_projection());

        assert_eq!(
            projection.refs,
            BTreeMap::from([("sessionId".to_string(), "7".to_string())])
        );
        assert_eq!(
            projection.metadata,
            json!({ "terminalType": "local_terminal" })
        );
    }

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

    #[test]
    fn ide_projection_separates_editor_tab_from_workspace_identity() {
        let projection = ide_workspace_target_projection(AiIdeTargetInput {
            node_id: "node-a".to_string(),
            connection_id: Some("connection-a".to_string()),
            active_editor_tab_id: Some("editor-a".to_string()),
            project_root_path: Some("/srv/project".to_string()),
            project_name: Some("Project".to_string()),
        });

        assert_eq!(projection.id, "ide-workspace:node-a");
        assert_eq!(projection.refs["tabId"], "editor-a");
        assert_eq!(projection.metadata["rootPath"], "/srv/project");
        assert_eq!(projection.metadata["activeTabId"], "editor-a");
    }

    #[test]
    fn raw_socket_projection_describes_transport_without_runtime_types() {
        let tcp = AiRawTcpTargetInput {
            endpoint_label: "socket.internal:9000".to_string(),
            host: "socket.internal".to_string(),
            port: 9000,
            line_ending: "lf".to_string(),
            display_mode: "text".to_string(),
            send_mode: "text".to_string(),
            tls_enabled: true,
            tls_verification: "system".to_string(),
            tls_server_name: None,
        };
        let udp = AiRawUdpTargetInput {
            remote_endpoint_label: "udp.internal:8125".to_string(),
            remote_host: "udp.internal".to_string(),
            remote_port: 8125,
            local_bind_host: Some("127.0.0.1".to_string()),
            local_bind_port: 0,
            line_ending: "none".to_string(),
            display_mode: "mixed".to_string(),
            send_mode: "hex".to_string(),
        };

        assert_eq!(raw_tcp_terminal_label(&tcp), "TLS socket.internal:9000");
        assert_eq!(raw_tcp_terminal_metadata(&tcp)["tls"]["enabled"], true);
        assert_eq!(raw_udp_terminal_label(&udp), "UDP udp.internal:8125");
        assert_eq!(raw_udp_terminal_metadata(&udp)["remotePort"], 8125);
    }
}
