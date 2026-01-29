//! Keyboard-Interactive Authentication (2FA) Support
//!
//! This module provides a completely isolated 2FA authentication flow that:
//! - Does NOT affect existing password/key authentication
//! - Uses event-driven IPC for frontend prompts
//! - Has strict timeout protection (60s) to prevent deadlocks
//! - Cleans up resources immediately on failure/cancel
//!
//! # Architecture
//!
//! ```text
//! Frontend                              Backend
//!    │                                     │
//!    │──── ssh_connect_kbi ───────────────▶│ Start KBI flow
//!    │                                     │
//!    │◀─── ssh_kbi_prompt event ───────────│ InfoRequest from server
//!    │                                     │
//!    │──── ssh_kbi_respond command ───────▶│ User responses
//!    │                                     │
//!    │◀─── ssh_kbi_result event ───────────│ Success/Failure
//! ```

use serde::{Deserialize, Serialize};
use parking_lot::Mutex;
use tokio::sync::oneshot;
use once_cell::sync::Lazy;

/// Keyboard-Interactive prompt from server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KbiPrompt {
    /// The prompt text to display
    pub prompt: String,
    /// true = show input (echo), false = mask input (password-style)
    pub echo: bool,
}

/// Event payload: Backend → Frontend
/// Sent when server requests keyboard-interactive input
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KbiPromptEvent {
    /// Unique ID for this authentication flow (routes responses)
    pub auth_flow_id: String,
    /// Display name from server (may be empty)
    pub name: String,
    /// Instructions from server (may be empty)
    pub instructions: String,
    /// List of prompts to display
    pub prompts: Vec<KbiPrompt>,
}

/// Event payload: Backend → Frontend
/// Sent when authentication completes (success or failure)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KbiResultEvent {
    /// The auth flow ID
    pub auth_flow_id: String,
    /// Whether authentication succeeded
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Session ID if successful (for subsequent terminal creation)
    pub session_id: Option<String>,
    /// WebSocket port for terminal connection
    pub ws_port: Option<u16>,
    /// WebSocket token for authentication
    pub ws_token: Option<String>,
}

/// Command payload: Frontend → Backend
/// User's responses to prompts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KbiRespondRequest {
    /// Must match the auth_flow_id from KbiPromptEvent
    pub auth_flow_id: String,
    /// Responses in same order as prompts (length must match)
    pub responses: Vec<String>,
}

/// Command payload: Frontend → Backend
/// User cancelled the authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KbiCancelRequest {
    /// The auth flow to cancel
    pub auth_flow_id: String,
}

// ============================================================================
// Pending Request Registry
// ============================================================================

/// Internal: pending response channel for an auth flow
struct PendingRequest {
    sender: oneshot::Sender<Result<Vec<String>, KbiError>>,
}

/// Error types for KBI flow
#[derive(Debug, Clone)]
pub enum KbiError {
    /// User cancelled the authentication
    Cancelled,
    /// Timeout waiting for user input
    Timeout,
    /// Auth flow not found (already completed or invalid ID)
    FlowNotFound,
    /// Channel communication error
    ChannelError(String),
}

impl std::fmt::Display for KbiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KbiError::Cancelled => write!(f, "Authentication cancelled by user"),
            KbiError::Timeout => write!(f, "Authentication timeout (60s)"),
            KbiError::FlowNotFound => write!(f, "Authentication flow not found"),
            KbiError::ChannelError(e) => write!(f, "Channel error: {}", e),
        }
    }
}

impl std::error::Error for KbiError {}

/// Global registry of pending KBI requests awaiting frontend response
static PENDING_REQUESTS: Lazy<Mutex<std::collections::HashMap<String, PendingRequest>>> =
    Lazy::new(|| Mutex::new(std::collections::HashMap::new()));

/// Register a new pending request and return the receiver
///
/// Called by backend when it needs to wait for frontend input
pub fn register_pending(auth_flow_id: String) -> oneshot::Receiver<Result<Vec<String>, KbiError>> {
    let (tx, rx) = oneshot::channel();
    let mut pending = PENDING_REQUESTS.lock();
    pending.insert(auth_flow_id, PendingRequest { sender: tx });
    rx
}

/// Complete a pending request with user responses
///
/// Called by Tauri command when frontend submits responses
pub fn complete_pending(auth_flow_id: &str, responses: Vec<String>) -> Result<(), KbiError> {
    let mut pending = PENDING_REQUESTS.lock();
    let request = pending.remove(auth_flow_id).ok_or(KbiError::FlowNotFound)?;
    
    // Send responses (ignore error if receiver dropped - means flow was cancelled/timed out)
    let _ = request.sender.send(Ok(responses));
    Ok(())
}

/// Cancel a pending request
///
/// Called by Tauri command when user closes the dialog
pub fn cancel_pending(auth_flow_id: &str) -> Result<(), KbiError> {
    let mut pending = PENDING_REQUESTS.lock();
    let request = pending.remove(auth_flow_id).ok_or(KbiError::FlowNotFound)?;
    
    // Send cancellation
    let _ = request.sender.send(Err(KbiError::Cancelled));
    Ok(())
}

/// Cleanup a pending request (called on timeout or error)
pub fn cleanup_pending(auth_flow_id: &str) {
    let mut pending = PENDING_REQUESTS.lock();
    pending.remove(auth_flow_id);
}

/// Get count of pending requests (for debugging/monitoring)
pub fn pending_count() -> usize {
    PENDING_REQUESTS.lock().len()
}

// ============================================================================
// Event Names (constants for type safety)
// ============================================================================

/// Event name for KBI prompt requests
pub const EVENT_KBI_PROMPT: &str = "ssh_kbi_prompt";

/// Event name for KBI results
pub const EVENT_KBI_RESULT: &str = "ssh_kbi_result";
