//! Session Reconnection Module
//!
//! Provides silent reconnection capabilities for SSH sessions.
//! When a connection drops, the system will attempt to reconnect
//! automatically while preserving the terminal state.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

use super::types::SessionConfig;

/// Reconnection configuration
#[derive(Debug, Clone)]
pub struct ReconnectConfig {
    /// Maximum number of reconnection attempts
    pub max_attempts: u32,
    /// Initial delay before first reconnection attempt (ms)
    pub initial_delay_ms: u64,
    /// Maximum delay between attempts (ms)
    pub max_delay_ms: u64,
    /// Multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Whether to enable automatic reconnection
    pub enabled: bool,
}

impl Default for ReconnectConfig {
    fn default() -> Self {
        Self {
            max_attempts: 5,
            initial_delay_ms: 1000, // 1 second
            max_delay_ms: 30000,    // 30 seconds
            backoff_multiplier: 1.5,
            enabled: true,
        }
    }
}

/// Reconnection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ReconnectState {
    /// Not attempting reconnection
    Idle = 0,
    /// Waiting before next attempt
    Waiting = 1,
    /// Currently attempting to reconnect
    Attempting = 2,
    /// Successfully reconnected
    Reconnected = 3,
    /// All attempts exhausted
    Failed = 4,
    /// Reconnection cancelled by user
    Cancelled = 5,
}

impl ReconnectState {
    fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Idle,
            1 => Self::Waiting,
            2 => Self::Attempting,
            3 => Self::Reconnected,
            4 => Self::Failed,
            5 => Self::Cancelled,
            _ => Self::Idle,
        }
    }
}

/// Events emitted during reconnection
#[derive(Debug, Clone)]
pub enum ReconnectEvent {
    /// Starting reconnection process
    Starting { session_id: String },
    /// Waiting before next attempt
    Waiting {
        session_id: String,
        delay_ms: u64,
        attempt: u32,
    },
    /// Attempting to reconnect
    Attempting {
        session_id: String,
        attempt: u32,
        max_attempts: u32,
    },
    /// Reconnection successful
    Success { session_id: String, attempt: u32 },
    /// Reconnection attempt failed
    AttemptFailed {
        session_id: String,
        attempt: u32,
        error: String,
    },
    /// All attempts exhausted
    Failed {
        session_id: String,
        total_attempts: u32,
    },
    /// Reconnection cancelled
    Cancelled { session_id: String },
}

/// Reconnection manager for a single session
pub struct SessionReconnector {
    /// Session ID
    session_id: String,
    /// Original session config for reconnection
    config: SessionConfig,
    /// Reconnection settings
    reconnect_config: ReconnectConfig,
    /// Current attempt number
    attempt_count: AtomicU32,
    /// Current state
    state: AtomicU8,
    /// Flag to cancel reconnection
    cancelled: AtomicBool,
    /// Event sender
    event_tx: Option<mpsc::Sender<ReconnectEvent>>,
}

impl SessionReconnector {
    /// Create a new reconnector for a session
    pub fn new(
        session_id: String,
        config: SessionConfig,
        reconnect_config: ReconnectConfig,
    ) -> Self {
        Self {
            session_id,
            config,
            reconnect_config,
            attempt_count: AtomicU32::new(0),
            state: AtomicU8::new(ReconnectState::Idle as u8),
            cancelled: AtomicBool::new(false),
            event_tx: None,
        }
    }

    /// Set event sender for monitoring reconnection progress
    pub fn with_event_sender(mut self, tx: mpsc::Sender<ReconnectEvent>) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Get current state
    pub fn state(&self) -> ReconnectState {
        ReconnectState::from_u8(self.state.load(Ordering::Acquire))
    }

    /// Get current attempt count
    pub fn attempt_count(&self) -> u32 {
        self.attempt_count.load(Ordering::SeqCst)
    }

    /// Cancel ongoing reconnection
    pub fn cancel(&self) {
        info!("Cancelling reconnection for session {}", self.session_id);
        self.cancelled.store(true, Ordering::SeqCst);
    }

    /// Check if reconnection is cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Reset reconnector for reuse
    pub fn reset(&self) {
        self.attempt_count.store(0, Ordering::SeqCst);
        self.cancelled.store(false, Ordering::SeqCst);
        self.state.store(ReconnectState::Idle as u8, Ordering::Release);
    }

    /// Calculate delay for current attempt using exponential backoff
    fn calculate_delay(&self, attempt: u32) -> u64 {
        let base_delay = self.reconnect_config.initial_delay_ms as f64;
        let multiplier = self.reconnect_config.backoff_multiplier;
        let delay = base_delay * multiplier.powi(attempt as i32 - 1);
        delay.min(self.reconnect_config.max_delay_ms as f64) as u64
    }

    /// Send event if event sender is configured
    async fn emit_event(&self, event: ReconnectEvent) {
        if let Some(ref tx) = self.event_tx {
            let _ = tx.send(event).await;
        }
    }

    /// Attempt reconnection with exponential backoff
    ///
    /// Returns Ok(()) if reconnection succeeds, Err if all attempts fail.
    /// The actual connection logic should be provided by the caller via the
    /// `connect_fn` closure.
    pub async fn attempt_reconnection<F, Fut>(
        &self,
        mut connect_fn: F,
    ) -> Result<(), ReconnectError>
    where
        F: FnMut(&SessionConfig) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        if !self.reconnect_config.enabled {
            return Err(ReconnectError::Disabled);
        }

        // Reset state
        self.state.store(ReconnectState::Idle as u8, Ordering::Release);
        self.attempt_count.store(0, Ordering::SeqCst);
        self.cancelled.store(false, Ordering::SeqCst);

        self.emit_event(ReconnectEvent::Starting {
            session_id: self.session_id.clone(),
        })
        .await;

        let max_attempts = self.reconnect_config.max_attempts;

        for attempt in 1..=max_attempts {
            // Check if cancelled
            if self.is_cancelled() {
                self.state.store(ReconnectState::Cancelled as u8, Ordering::Release);
                self.emit_event(ReconnectEvent::Cancelled {
                    session_id: self.session_id.clone(),
                })
                .await;
                return Err(ReconnectError::Cancelled);
            }

            // Calculate and apply delay (except for first attempt)
            if attempt > 1 {
                let delay_ms = self.calculate_delay(attempt);
                self.state.store(ReconnectState::Waiting as u8, Ordering::Release);

                self.emit_event(ReconnectEvent::Waiting {
                    session_id: self.session_id.clone(),
                    delay_ms,
                    attempt,
                })
                .await;

                info!(
                    "Session {}: waiting {}ms before reconnect attempt {}/{}",
                    self.session_id, delay_ms, attempt, max_attempts
                );

                // Wait in small increments to allow cancellation
                let delay_duration = Duration::from_millis(delay_ms);
                let check_interval = Duration::from_millis(100);
                let mut elapsed = Duration::ZERO;

                while elapsed < delay_duration {
                    if self.is_cancelled() {
                        self.state.store(ReconnectState::Cancelled as u8, Ordering::Release);
                        return Err(ReconnectError::Cancelled);
                    }
                    sleep(check_interval.min(delay_duration - elapsed)).await;
                    elapsed += check_interval;
                }
            }

            // Attempt connection
            self.attempt_count.store(attempt, Ordering::SeqCst);
            self.state.store(ReconnectState::Attempting as u8, Ordering::Release);

            self.emit_event(ReconnectEvent::Attempting {
                session_id: self.session_id.clone(),
                attempt,
                max_attempts,
            })
            .await;

            info!(
                "Session {}: reconnection attempt {}/{}",
                self.session_id, attempt, max_attempts
            );

            match connect_fn(&self.config).await {
                Ok(()) => {
                    self.state.store(ReconnectState::Reconnected as u8, Ordering::Release);

                    self.emit_event(ReconnectEvent::Success {
                        session_id: self.session_id.clone(),
                        attempt,
                    })
                    .await;

                    info!(
                        "Session {}: reconnection successful on attempt {}",
                        self.session_id, attempt
                    );

                    return Ok(());
                }
                Err(error) => {
                    self.emit_event(ReconnectEvent::AttemptFailed {
                        session_id: self.session_id.clone(),
                        attempt,
                        error: error.clone(),
                    })
                    .await;

                    warn!(
                        "Session {}: reconnection attempt {} failed: {}",
                        self.session_id, attempt, error
                    );
                }
            }
        }

        // All attempts exhausted
        self.state.store(ReconnectState::Failed as u8, Ordering::Release);

        self.emit_event(ReconnectEvent::Failed {
            session_id: self.session_id.clone(),
            total_attempts: max_attempts,
        })
        .await;

        error!(
            "Session {}: reconnection failed after {} attempts",
            self.session_id, max_attempts
        );

        Err(ReconnectError::MaxAttemptsReached(max_attempts))
    }
}

/// Errors during reconnection
#[derive(Debug, Clone)]
pub enum ReconnectError {
    /// Reconnection is disabled
    Disabled,
    /// Reconnection was cancelled
    Cancelled,
    /// Maximum attempts reached
    MaxAttemptsReached(u32),
    /// Session not found
    SessionNotFound(String),
}

impl std::fmt::Display for ReconnectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "Reconnection is disabled"),
            Self::Cancelled => write!(f, "Reconnection was cancelled"),
            Self::MaxAttemptsReached(n) => {
                write!(f, "Maximum reconnection attempts ({}) reached", n)
            }
            Self::SessionNotFound(id) => write!(f, "Session {} not found", id),
        }
    }
}

impl std::error::Error for ReconnectError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reconnect_config_defaults() {
        let config = ReconnectConfig::default();
        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_delay_ms, 1000);
        assert!(config.enabled);
    }

    #[test]
    fn test_calculate_delay() {
        let config = SessionConfig {
            host: "test".to_string(),
            port: 22,
            username: "user".to_string(),
            auth: crate::session::types::AuthMethod::password("test123"),
            name: None,
            color: None,
            cols: 80,
            rows: 24,
        };

        let reconnector = SessionReconnector::new(
            "test-session".to_string(),
            config,
            ReconnectConfig::default(),
        );

        // First attempt: 1000ms
        assert_eq!(reconnector.calculate_delay(1), 1000);

        // Second attempt: 1500ms (1000 * 1.5)
        assert_eq!(reconnector.calculate_delay(2), 1500);

        // Third attempt: 2250ms (1000 * 1.5^2)
        assert_eq!(reconnector.calculate_delay(3), 2250);
    }

    #[tokio::test]
    async fn test_cancel_reconnection() {
        let config = SessionConfig {
            host: "test".to_string(),
            port: 22,
            username: "user".to_string(),
            auth: crate::session::types::AuthMethod::password("test123"),
            name: None,
            color: None,
            cols: 80,
            rows: 24,
        };

        let reconnector = SessionReconnector::new(
            "test-session".to_string(),
            config,
            ReconnectConfig::default(),
        );

        assert!(!reconnector.is_cancelled());
        reconnector.cancel();
        assert!(reconnector.is_cancelled());
    }
}
