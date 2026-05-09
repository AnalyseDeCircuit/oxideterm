// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

use crate::{DetectedPort, ForwardStats, ForwardStatus};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ForwardEvent {
    StatusChanged {
        forward_id: String,
        session_id: String,
        status: ForwardStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    StatsUpdated {
        forward_id: String,
        session_id: String,
        stats: ForwardStats,
    },
    SessionSuspended {
        session_id: String,
        forward_ids: Vec<String>,
    },
    PortDetected {
        connection_id: String,
        new_ports: Vec<DetectedPort>,
        closed_ports: Vec<DetectedPort>,
        all_ports: Vec<DetectedPort>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_event_uses_tauri_camel_case_tag() {
        let event = ForwardEvent::StatusChanged {
            forward_id: "forward-1".to_string(),
            session_id: "session-1".to_string(),
            status: ForwardStatus::Active,
            error: None,
        };
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("statusChanged"));
        assert!(json.contains("forward-1"));
        assert!(!json.contains("error"));
    }

    #[test]
    fn error_status_serializes_like_tauri_status_enum() {
        let event = ForwardEvent::StatusChanged {
            forward_id: "forward-1".to_string(),
            session_id: "session-1".to_string(),
            status: ForwardStatus::Error,
            error: Some("connection lost".to_string()),
        };
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("\"status\":\"error\""));
        assert!(json.contains("\"error\":\"connection lost\""));
    }

    #[test]
    fn session_suspended_event_carries_all_forward_ids() {
        let event = ForwardEvent::SessionSuspended {
            session_id: "session-1".to_string(),
            forward_ids: vec!["one".to_string(), "two".to_string()],
        };
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains("sessionSuspended"));
        assert!(json.contains("one"));
        assert!(json.contains("two"));
    }
}
