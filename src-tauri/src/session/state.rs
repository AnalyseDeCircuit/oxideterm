// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Session State Machine
//!
//! Defines the valid state transitions for SSH sessions:
//!
//! ```text
//! ┌──────────────┐     connect()     ┌──────────────┐
//! │ Disconnected │ ─────────────────► │  Connecting  │
//! └──────────────┘                    └──────┬───────┘
//!        ▲                                   │
//!        │                          success / failure
//!        │                                   │
//!        │ timeout/error    ┌────────────────┴────────────────┐
//!        │                  ▼                                  ▼
//!        │          ┌──────────────┐                  ┌──────────────┐
//!        └──────────│  Connected   │                  │    Error     │
//!                   └──────┬───────┘                  └──────────────┘
//!                          │
//!                    disconnect()
//!                          │
//!                          ▼
//!                   ┌──────────────┐
//!                   │Disconnecting │
//!                   └──────────────┘
//! ```

use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::Instant;

/// Session states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Initial state, not connected
    #[default]
    Disconnected,
    /// Attempting to establish connection (SSH handshake + auth)
    Connecting,
    /// Successfully connected and ready for I/O
    Connected,
    /// Gracefully closing the connection
    Disconnecting,
    /// Connection failed with an error
    Error,
}

impl fmt::Display for SessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => write!(f, "disconnected"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::Disconnecting => write!(f, "disconnecting"),
            Self::Error => write!(f, "error"),
        }
    }
}

/// State machine for session lifecycle management
#[derive(Debug)]
pub struct SessionStateMachine {
    state: SessionState,
    error_message: Option<String>,
    state_changed_at: Instant,
    transition_count: u32,
}

impl Default for SessionStateMachine {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionStateMachine {
    /// Create a new state machine in Disconnected state
    pub fn new() -> Self {
        Self {
            state: SessionState::Disconnected,
            error_message: None,
            state_changed_at: Instant::now(),
            transition_count: 0,
        }
    }

    /// Get current state
    pub fn state(&self) -> SessionState {
        self.state
    }

    /// Get error message if in Error state
    pub fn error(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Get time elapsed since last state change
    pub fn time_in_state(&self) -> std::time::Duration {
        self.state_changed_at.elapsed()
    }

    /// Get total number of state transitions
    pub fn transition_count(&self) -> u32 {
        self.transition_count
    }

    /// Attempt to transition to Connecting state
    pub fn start_connecting(&mut self) -> Result<(), StateTransitionError> {
        match self.state {
            SessionState::Disconnected | SessionState::Error => {
                self.transition_to(SessionState::Connecting);
                self.error_message = None;
                Ok(())
            }
            _ => Err(StateTransitionError::InvalidTransition {
                from: self.state,
                to: SessionState::Connecting,
            }),
        }
    }

    /// Transition to Connected state on success
    pub fn connect_success(&mut self) -> Result<(), StateTransitionError> {
        match self.state {
            SessionState::Connecting => {
                self.transition_to(SessionState::Connected);
                Ok(())
            }
            _ => Err(StateTransitionError::InvalidTransition {
                from: self.state,
                to: SessionState::Connected,
            }),
        }
    }

    /// Transition to Error state on failure
    pub fn connect_failed(&mut self, error: String) -> Result<(), StateTransitionError> {
        match self.state {
            SessionState::Connecting => {
                self.transition_to(SessionState::Error);
                self.error_message = Some(error);
                Ok(())
            }
            _ => Err(StateTransitionError::InvalidTransition {
                from: self.state,
                to: SessionState::Error,
            }),
        }
    }

    /// Start disconnection process
    pub fn start_disconnecting(&mut self) -> Result<(), StateTransitionError> {
        match self.state {
            SessionState::Connected | SessionState::Connecting => {
                self.transition_to(SessionState::Disconnecting);
                Ok(())
            }
            _ => Err(StateTransitionError::InvalidTransition {
                from: self.state,
                to: SessionState::Disconnecting,
            }),
        }
    }

    /// Complete disconnection
    pub fn disconnect_complete(&mut self) -> Result<(), StateTransitionError> {
        match self.state {
            SessionState::Disconnecting => {
                self.transition_to(SessionState::Disconnected);
                Ok(())
            }
            // Also allow direct transition from Connected in case of abrupt disconnect
            SessionState::Connected => {
                self.transition_to(SessionState::Disconnected);
                Ok(())
            }
            _ => Err(StateTransitionError::InvalidTransition {
                from: self.state,
                to: SessionState::Disconnected,
            }),
        }
    }

    /// Set error state (can be called from any state)
    pub fn set_error(&mut self, error: String) {
        self.transition_to(SessionState::Error);
        self.error_message = Some(error);
    }

    /// Reset to disconnected state (force reset)
    pub fn reset(&mut self) {
        self.transition_to(SessionState::Disconnected);
        self.error_message = None;
    }

    /// Check if session is in a terminal state (disconnected or error)
    pub fn is_terminal(&self) -> bool {
        matches!(self.state, SessionState::Disconnected | SessionState::Error)
    }

    /// Check if session is active (connected or connecting)
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            SessionState::Connecting | SessionState::Connected
        )
    }

    fn transition_to(&mut self, new_state: SessionState) {
        tracing::debug!(
            "Session state transition: {} -> {} (count: {})",
            self.state,
            new_state,
            self.transition_count + 1
        );
        self.state = new_state;
        self.state_changed_at = Instant::now();
        self.transition_count += 1;
    }
}

/// Error type for invalid state transitions
#[derive(Debug, Clone, thiserror::Error)]
pub enum StateTransitionError {
    #[error("Invalid state transition from {from} to {to}")]
    InvalidTransition {
        from: SessionState,
        to: SessionState,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_happy_path() {
        let mut sm = SessionStateMachine::new();
        assert_eq!(sm.state(), SessionState::Disconnected);

        sm.start_connecting().unwrap();
        assert_eq!(sm.state(), SessionState::Connecting);

        sm.connect_success().unwrap();
        assert_eq!(sm.state(), SessionState::Connected);

        sm.start_disconnecting().unwrap();
        assert_eq!(sm.state(), SessionState::Disconnecting);

        sm.disconnect_complete().unwrap();
        assert_eq!(sm.state(), SessionState::Disconnected);
    }

    #[test]
    fn test_connect_failure() {
        let mut sm = SessionStateMachine::new();
        sm.start_connecting().unwrap();
        sm.connect_failed("Connection refused".to_string()).unwrap();

        assert_eq!(sm.state(), SessionState::Error);
        assert_eq!(sm.error(), Some("Connection refused"));
    }

    #[test]
    fn test_invalid_transition() {
        let mut sm = SessionStateMachine::new();
        // Cannot go directly to Connected
        assert!(sm.connect_success().is_err());
    }

    #[test]
    fn test_error_to_connecting() {
        let mut sm = SessionStateMachine::new();
        sm.start_connecting().unwrap();
        sm.connect_failed("timeout".into()).unwrap();
        assert_eq!(sm.state(), SessionState::Error);

        // Retry: Error → Connecting is allowed
        sm.start_connecting().unwrap();
        assert_eq!(sm.state(), SessionState::Connecting);
        assert!(sm.error().is_none());
    }

    #[test]
    fn test_cannot_connect_failed_from_disconnected() {
        let mut sm = SessionStateMachine::new();
        assert!(sm.connect_failed("err".into()).is_err());
    }

    #[test]
    fn test_cannot_disconnect_from_disconnected() {
        let mut sm = SessionStateMachine::new();
        assert!(sm.start_disconnecting().is_err());
    }

    #[test]
    fn test_cannot_disconnect_complete_from_connecting() {
        let mut sm = SessionStateMachine::new();
        sm.start_connecting().unwrap();
        assert!(sm.disconnect_complete().is_err());
    }

    #[test]
    fn test_disconnect_from_connecting() {
        // Abort connection attempt
        let mut sm = SessionStateMachine::new();
        sm.start_connecting().unwrap();
        sm.start_disconnecting().unwrap();
        assert_eq!(sm.state(), SessionState::Disconnecting);
    }

    #[test]
    fn test_abrupt_disconnect_from_connected() {
        let mut sm = SessionStateMachine::new();
        sm.start_connecting().unwrap();
        sm.connect_success().unwrap();
        // Direct disconnect_complete from Connected (abrupt)
        sm.disconnect_complete().unwrap();
        assert_eq!(sm.state(), SessionState::Disconnected);
    }

    #[test]
    fn test_set_error_from_any_state() {
        let mut sm = SessionStateMachine::new();
        sm.set_error("fatal".into());
        assert_eq!(sm.state(), SessionState::Error);
        assert_eq!(sm.error(), Some("fatal"));

        sm.start_connecting().unwrap();
        sm.connect_success().unwrap();
        sm.set_error("connection lost".into());
        assert_eq!(sm.state(), SessionState::Error);
    }

    #[test]
    fn test_reset() {
        let mut sm = SessionStateMachine::new();
        sm.start_connecting().unwrap();
        sm.connect_success().unwrap();
        sm.set_error("lost".into());

        sm.reset();
        assert_eq!(sm.state(), SessionState::Disconnected);
        assert!(sm.error().is_none());
    }

    #[test]
    fn test_is_terminal() {
        let mut sm = SessionStateMachine::new();
        assert!(sm.is_terminal()); // Disconnected

        sm.start_connecting().unwrap();
        assert!(!sm.is_terminal());

        sm.connect_success().unwrap();
        assert!(!sm.is_terminal());

        sm.set_error("err".into());
        assert!(sm.is_terminal()); // Error
    }

    #[test]
    fn test_is_active() {
        let mut sm = SessionStateMachine::new();
        assert!(!sm.is_active());

        sm.start_connecting().unwrap();
        assert!(sm.is_active()); // Connecting

        sm.connect_success().unwrap();
        assert!(sm.is_active()); // Connected

        sm.start_disconnecting().unwrap();
        assert!(!sm.is_active()); // Disconnecting
    }

    #[test]
    fn test_transition_count() {
        let mut sm = SessionStateMachine::new();
        assert_eq!(sm.transition_count(), 0);

        sm.start_connecting().unwrap();
        assert_eq!(sm.transition_count(), 1);

        sm.connect_success().unwrap();
        assert_eq!(sm.transition_count(), 2);
    }

    #[test]
    fn test_time_in_state() {
        let sm = SessionStateMachine::new();
        let d = sm.time_in_state();
        // Should be very small (just created)
        assert!(d.as_secs() < 1);
    }

    #[test]
    fn test_session_state_display() {
        assert_eq!(SessionState::Disconnected.to_string(), "disconnected");
        assert_eq!(SessionState::Connecting.to_string(), "connecting");
        assert_eq!(SessionState::Connected.to_string(), "connected");
        assert_eq!(SessionState::Disconnecting.to_string(), "disconnecting");
        assert_eq!(SessionState::Error.to_string(), "error");
    }

    #[test]
    fn test_default_state() {
        let sm = SessionStateMachine::default();
        assert_eq!(sm.state(), SessionState::Disconnected);
    }
}
