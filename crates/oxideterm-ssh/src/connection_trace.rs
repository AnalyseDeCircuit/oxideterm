use std::collections::HashMap;

use oxideterm_backend_classification::{BackendErrorClass, classify_message};

use crate::NodeId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTraceStage {
    Queued,
    Preparing,
    OpeningTransport,
    SshHandshake,
    HostKey,
    Authentication,
    Pty,
    ShellReady,
    Ready,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTraceStatus {
    Running,
    Ready,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionTraceMode {
    Connect,
    Reconnect,
}

#[derive(Clone, Debug)]
pub struct ConnectionTraceEvent {
    pub attempt_id: String,
    pub node_id: NodeId,
    pub stage: ConnectionTraceStage,
    pub status: ConnectionTraceStatus,
    pub progress: f32,
    pub elapsed_ms: u64,
    pub detail: Option<String>,
    pub label: Option<String>,
    pub step_index: Option<u32>,
    pub total_steps: Option<u32>,
    pub mode: ConnectionTraceMode,
}

#[derive(Clone, Debug)]
pub struct ConnectionTracePlan {
    pub attempt_id: String,
    pub mode: ConnectionTraceMode,
    pub node_ids: Vec<NodeId>,
}

#[derive(Clone, Debug)]
struct ConnectionTraceNodeContext {
    attempt_id: String,
    label: Option<String>,
    step_index: Option<u32>,
    total_steps: Option<u32>,
    mode: ConnectionTraceMode,
}

#[derive(Default)]
pub struct ConnectionTraceState {
    attempt_sequence: u64,
    nodes: HashMap<NodeId, ConnectionTraceNodeContext>,
}

impl ConnectionTraceState {
    pub fn plan_for_path(
        &mut self,
        mode: ConnectionTraceMode,
        path: &[(NodeId, bool)],
    ) -> Option<ConnectionTracePlan> {
        let start_index = path.iter().position(|(_, ready)| !ready)?;
        Some(ConnectionTracePlan {
            attempt_id: self.next_attempt_id(),
            mode,
            node_ids: path[start_index..]
                .iter()
                .map(|(node_id, _)| node_id.clone())
                .collect(),
        })
    }

    pub fn begin(
        &mut self,
        node_id: NodeId,
        label: Option<String>,
        plan: Option<&ConnectionTracePlan>,
    ) {
        let (attempt_id, mode, step_index, total_steps) = plan
            .and_then(|plan| {
                let step = plan
                    .node_ids
                    .iter()
                    .position(|candidate| candidate == &node_id)?;
                Some((
                    plan.attempt_id.clone(),
                    plan.mode,
                    (step + 1) as u32,
                    plan.node_ids.len() as u32,
                ))
            })
            .unwrap_or_else(|| (self.next_attempt_id(), ConnectionTraceMode::Connect, 1, 1));
        self.nodes.insert(
            node_id,
            ConnectionTraceNodeContext {
                attempt_id,
                label,
                step_index: Some(step_index),
                total_steps: Some(total_steps),
                mode,
            },
        );
    }

    pub fn event(
        &self,
        node_id: &NodeId,
        stage: ConnectionTraceStage,
        status: ConnectionTraceStatus,
        progress: f32,
        detail: Option<String>,
    ) -> Option<ConnectionTraceEvent> {
        let context = self.nodes.get(node_id)?;
        Some(ConnectionTraceEvent {
            attempt_id: context.attempt_id.clone(),
            node_id: node_id.clone(),
            stage,
            status,
            progress,
            elapsed_ms: 0,
            detail,
            label: context.label.clone(),
            step_index: context.step_index,
            total_steps: context.total_steps,
            mode: context.mode,
        })
    }

    pub fn contains(&self, node_id: &NodeId) -> bool {
        self.nodes.contains_key(node_id)
    }

    pub fn finish(&mut self, node_id: &NodeId) {
        self.nodes.remove(node_id);
    }

    pub fn next_attempt_id(&mut self) -> String {
        self.attempt_sequence = self.attempt_sequence.wrapping_add(1);
        format!("native-connection-{}", self.attempt_sequence)
    }
}

pub fn connection_trace_failure_stage(error: Option<&str>) -> ConnectionTraceStage {
    let Some(error) = error else {
        return ConnectionTraceStage::Authentication;
    };
    let error = error.to_ascii_lowercase();

    if error.contains("node not found")
        || error.contains("already connecting")
        || error.contains("already connected")
    {
        return ConnectionTraceStage::Preparing;
    }
    if error.contains("algorithm negotiation failed") {
        return ConnectionTraceStage::SshHandshake;
    }

    match classify_message(&error) {
        BackendErrorClass::Disconnected
        | BackendErrorClass::PortInUse
        | BackendErrorClass::Timeout => ConnectionTraceStage::OpeningTransport,
        BackendErrorClass::HostKey => ConnectionTraceStage::HostKey,
        BackendErrorClass::Auth
        | BackendErrorClass::Cancelled
        | BackendErrorClass::PermissionDenied
        | BackendErrorClass::Unsupported
        | BackendErrorClass::Conflict
        | BackendErrorClass::NotFound
        | BackendErrorClass::Other => ConnectionTraceStage::Authentication,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshAlgorithmDiagnosticKind {
    KeyExchange,
    HostKey,
    Cipher,
    Mac,
    Compression,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshAlgorithmNegotiationDiagnostic {
    pub kind: SshAlgorithmDiagnosticKind,
    pub client_algorithms: Vec<String>,
    pub server_algorithms: Vec<String>,
}

pub fn parse_algorithm_negotiation_error(error: &str) -> Option<SshAlgorithmNegotiationDiagnostic> {
    // Transport errors currently cross the app boundary as text, so decode only
    // the transport's stable display shape until deliveries become structured.
    let prefix = "SSH algorithm negotiation failed: no common ";
    let after_prefix = error.split_once(prefix)?.1;
    let (kind_text, after_kind) = after_prefix.split_once(" algorithm. Client offered: ")?;
    let kind = match kind_text {
        "key exchange" => SshAlgorithmDiagnosticKind::KeyExchange,
        "host key" => SshAlgorithmDiagnosticKind::HostKey,
        "cipher" => SshAlgorithmDiagnosticKind::Cipher,
        "MAC" => SshAlgorithmDiagnosticKind::Mac,
        "compression" => SshAlgorithmDiagnosticKind::Compression,
        _ => return None,
    };
    let (client_text, server_text) = after_kind.split_once("; server offered: ")?;
    Some(SshAlgorithmNegotiationDiagnostic {
        kind,
        client_algorithms: parse_debug_algorithm_list(client_text),
        server_algorithms: parse_debug_algorithm_list(server_text),
    })
}

fn parse_debug_algorithm_list(list: &str) -> Vec<String> {
    let mut algorithms = Vec::new();
    let mut current = String::new();
    let mut in_string = false;
    let mut escaping = false;
    for character in list.chars() {
        if in_string {
            if escaping {
                current.push(character);
                escaping = false;
            } else if character == '\\' {
                escaping = true;
            } else if character == '"' {
                algorithms.push(std::mem::take(&mut current));
                in_string = false;
            } else {
                current.push(character);
            }
        } else if character == '"' {
            in_string = true;
        }
    }
    algorithms
}

pub fn server_only_offers_ssh_rsa(algorithms: &[String]) -> bool {
    !algorithms.is_empty()
        && algorithms
            .iter()
            .all(|algorithm| algorithm == "ssh-rsa" || algorithm == "ssh-rsa-cert-v01@openssh.com")
}

pub fn server_offers_legacy_cipher(algorithms: &[String]) -> bool {
    algorithms.iter().any(|algorithm| {
        algorithm.contains("-cbc") || algorithm.contains("3des") || algorithm.contains("arcfour")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_starts_at_first_unready_node() {
        let mut state = ConnectionTraceState::default();
        let path = [
            (NodeId("parent".into()), true),
            (NodeId("target".into()), false),
        ];

        let plan = state
            .plan_for_path(ConnectionTraceMode::Connect, &path)
            .expect("unready target should produce a trace plan");

        assert_eq!(plan.node_ids, [NodeId("target".into())]);
    }

    #[test]
    fn parses_algorithm_negotiation_lists_from_transport_error() {
        let diagnostic = parse_algorithm_negotiation_error(
            "SSH algorithm negotiation failed: no common key exchange algorithm. Client offered: [\"curve25519-sha256\", \"diffie-hellman-group14-sha256\"]; server offered: [\"diffie-hellman-group1-sha1\"]",
        )
        .expect("algorithm diagnostic should parse");

        assert_eq!(diagnostic.kind, SshAlgorithmDiagnosticKind::KeyExchange);
        assert_eq!(
            diagnostic.client_algorithms,
            ["curve25519-sha256", "diffie-hellman-group14-sha256"]
        );
        assert_eq!(diagnostic.server_algorithms, ["diffie-hellman-group1-sha1"]);
    }

    #[test]
    fn failure_stage_preserves_transport_phase() {
        assert_eq!(
            connection_trace_failure_stage(Some("Connection failed: network unreachable")),
            ConnectionTraceStage::OpeningTransport
        );
        assert_eq!(
            connection_trace_failure_stage(Some("Host key changed for example.com")),
            ConnectionTraceStage::HostKey
        );
        assert_eq!(
            connection_trace_failure_stage(Some("Authentication failed: permission denied")),
            ConnectionTraceStage::Authentication
        );
    }
}
