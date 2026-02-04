//! Forward Event System
//!
//! Provides event emission for port forward status changes.
//! Events are emitted through Tauri's event system to the frontend.

use serde::{Deserialize, Serialize};
use tauri::Emitter;

use super::manager::{ForwardStats, ForwardStatus};

/// Forward event types emitted to frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ForwardEvent {
    /// Forward status changed (created, stopped, error, etc.)
    StatusChanged {
        forward_id: String,
        session_id: String,
        status: ForwardStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// Forward statistics updated (throttled, not every packet)
    StatsUpdated {
        forward_id: String,
        session_id: String,
        stats: ForwardStats,
    },
    /// All forwards for a session suspended due to SSH disconnect
    SessionSuspended {
        session_id: String,
        forward_ids: Vec<String>,
    },
}

/// Event emitter for forwarding module
///
/// Wraps Tauri's AppHandle to emit forward events.
/// Can be None for testing or when events are not needed.
#[derive(Clone)]
pub struct ForwardEventEmitter {
    app_handle: Option<tauri::AppHandle>,
    session_id: String,
}

impl ForwardEventEmitter {
    /// Create a new event emitter with Tauri AppHandle
    pub fn new(app_handle: tauri::AppHandle, session_id: String) -> Self {
        Self {
            app_handle: Some(app_handle),
            session_id,
        }
    }

    /// Create a no-op emitter (for testing or when events not needed)
    pub fn noop(session_id: String) -> Self {
        Self {
            app_handle: None,
            session_id,
        }
    }

    /// Emit a forward event
    pub fn emit(&self, event: ForwardEvent) {
        if let Some(ref handle) = self.app_handle {
            if let Err(e) = handle.emit("forward-event", &event) {
                tracing::warn!("Failed to emit forward event: {}", e);
            }
        }
    }

    /// Emit status changed event
    pub fn emit_status_changed(
        &self,
        forward_id: &str,
        status: ForwardStatus,
        error: Option<String>,
    ) {
        self.emit(ForwardEvent::StatusChanged {
            forward_id: forward_id.to_string(),
            session_id: self.session_id.clone(),
            status,
            error,
        });
    }

    /// Emit stats updated event
    pub fn emit_stats_updated(&self, forward_id: &str, stats: ForwardStats) {
        self.emit(ForwardEvent::StatsUpdated {
            forward_id: forward_id.to_string(),
            session_id: self.session_id.clone(),
            stats,
        });
    }

    /// Emit session suspended event (SSH disconnected)
    pub fn emit_session_suspended(&self, forward_ids: Vec<String>) {
        self.emit(ForwardEvent::SessionSuspended {
            session_id: self.session_id.clone(),
            forward_ids,
        });
    }

    /// Get session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

impl std::fmt::Debug for ForwardEventEmitter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ForwardEventEmitter")
            .field("session_id", &self.session_id)
            .field("has_app_handle", &self.app_handle.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_emitter() {
        let emitter = ForwardEventEmitter::noop("test-session".into());
        // Should not panic
        emitter.emit_status_changed("fwd-1", ForwardStatus::Active, None);
        emitter.emit_session_suspended(vec!["fwd-1".into(), "fwd-2".into()]);
    }

    #[test]
    fn test_event_serialization() {
        let event = ForwardEvent::StatusChanged {
            forward_id: "fwd-1".into(),
            session_id: "sess-1".into(),
            status: ForwardStatus::Active,
            error: None,
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("statusChanged"));
        assert!(json.contains("fwd-1"));
    }
}
