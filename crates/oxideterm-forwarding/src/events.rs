// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fmt,
    sync::{
        Arc,
        mpsc::{SendError, Sender},
    },
};

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

#[derive(Clone)]
pub struct ForwardEventDeliverySender {
    sender: Sender<ForwardEvent>,
    wake: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl ForwardEventDeliverySender {
    pub fn new(sender: Sender<ForwardEvent>) -> Self {
        Self { sender, wake: None }
    }

    pub fn with_wake(sender: Sender<ForwardEvent>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            sender,
            wake: Some(wake),
        }
    }

    pub fn send(&self, event: ForwardEvent) -> Result<(), SendError<ForwardEvent>> {
        self.sender.send(event)?;
        if let Some(wake) = &self.wake {
            wake();
        }
        Ok(())
    }
}

impl fmt::Debug for ForwardEventDeliverySender {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ForwardEventDeliverySender")
            .field("has_wake", &self.wake.is_some())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn delivery_sender_wakes_after_event_is_queued() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let woke = Arc::new(AtomicBool::new(false));
        let wake_flag = woke.clone();
        let sender = ForwardEventDeliverySender::with_wake(
            sender,
            Arc::new(move || wake_flag.store(true, Ordering::Release)),
        );

        sender
            .send(ForwardEvent::SessionSuspended {
                session_id: "session-1".to_string(),
                forward_ids: Vec::new(),
            })
            .unwrap();

        assert!(woke.load(Ordering::Acquire));
        assert!(matches!(
            receiver.try_recv().unwrap(),
            ForwardEvent::SessionSuspended { .. }
        ));
    }

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
