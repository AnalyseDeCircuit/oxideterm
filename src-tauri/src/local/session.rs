//! Local Terminal Session
//!
//! Manages a single local terminal session with PTY, data pump,
//! and WebSocket integration for frontend communication.

use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, MutexGuard};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::local::pty::{PtyConfig, PtyError, PtyHandle};
use crate::local::shell::ShellInfo;

/// Error type for session operations
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("PTY error: {0}")]
    PtyError(#[from] PtyError),

    #[error("Session not found: {0}")]
    NotFound(String),

    #[error("Session already closed")]
    AlreadyClosed,

    #[error("Channel send error")]
    ChannelError,
}

/// Events emitted by a local terminal session
#[derive(Debug, Clone)]
pub enum SessionEvent {
    /// Data received from PTY (to be sent to frontend)
    Data(Vec<u8>),
    /// Session has closed
    Closed(Option<i32>), // exit code if available
}

/// A local terminal session
pub struct LocalTerminalSession {
    /// Unique session ID
    pub id: String,
    /// Shell being used
    pub shell: ShellInfo,
    /// Current terminal size
    cols: u16,
    rows: u16,
    /// The PTY instance (wrapped for thread safety)
    pty: Option<Arc<PtyHandle>>,
    /// Whether the session is running
    running: Arc<AtomicBool>,
    /// Channel to send data to the PTY
    input_tx: Option<mpsc::Sender<Vec<u8>>>,
    /// Task handle for the data pump
    _data_pump_handle: Option<tokio::task::JoinHandle<()>>,
}

// Implement Send + Sync manually since we've made PtyHandle thread-safe
unsafe impl Send for LocalTerminalSession {}
unsafe impl Sync for LocalTerminalSession {}

impl LocalTerminalSession {
    /// Create a new local terminal session
    pub fn new(shell: ShellInfo, cols: u16, rows: u16) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            shell,
            cols,
            rows,
            pty: None,
            running: Arc::new(AtomicBool::new(false)),
            input_tx: None,
            _data_pump_handle: None,
        }
    }

    /// Start the session with a PTY
    pub async fn start(
        &mut self,
        cwd: Option<std::path::PathBuf>,
        event_tx: mpsc::Sender<SessionEvent>,
    ) -> Result<(), SessionError> {
        self.start_with_options(cwd, event_tx, true, false, None).await
    }
    
    /// Start the session with extended options for profile and Oh My Posh
    pub async fn start_with_options(
        &mut self,
        cwd: Option<std::path::PathBuf>,
        event_tx: mpsc::Sender<SessionEvent>,
        load_profile: bool,
        oh_my_posh_enabled: bool,
        oh_my_posh_theme: Option<String>,
    ) -> Result<(), SessionError> {
        let config = PtyConfig {
            cols: self.cols,
            rows: self.rows,
            shell: self.shell.clone(),
            cwd,
            env: vec![],
            load_profile,
            oh_my_posh_enabled,
            oh_my_posh_theme,
        };

        let pty = Arc::new(PtyHandle::new(config)?);
        let reader = pty.clone_reader();
        let writer = pty.clone_writer();

        self.pty = Some(pty);
        self.running.store(true, Ordering::SeqCst);

        // Create input channel for writing to PTY
        let (input_tx, mut input_rx) = mpsc::channel::<Vec<u8>>(256);
        self.input_tx = Some(input_tx);

        // Spawn write pump (frontend -> PTY)
        let running_write = self.running.clone();
        let writer_clone = writer.clone();
        tokio::spawn(async move {
            while running_write.load(Ordering::SeqCst) {
                match input_rx.recv().await {
                    Some(data) => {
                        if let Ok(mut w) = writer_clone.lock() {
                            let w: &mut Box<dyn Write + Send> = &mut *w;
                            if let Err(e) = w.write_all(&data) {
                                tracing::error!("Failed to write to PTY: {}", e);
                                break;
                            }
                            if let Err(e) = w.flush() {
                                tracing::error!("Failed to flush PTY: {}", e);
                                break;
                            }
                        } else {
                            tracing::error!("Failed to acquire writer lock");
                            break;
                        }
                    }
                    None => break,
                }
            }
            tracing::debug!("Write pump terminated");
        });

        // Spawn read pump (PTY -> frontend)
        let running_read = self.running.clone();
        let session_id = self.id.clone();
        let handle = tokio::task::spawn_blocking(move || {
            let rt = tokio::runtime::Handle::current();
            let mut buf = [0u8; 8192]; // 8KB buffer for efficiency
            let mut remainder: Vec<u8> = Vec::new(); // UTF-8 remainder buffer

            loop {
                if !running_read.load(Ordering::SeqCst) {
                    tracing::debug!("Read pump: session stopped");
                    break;
                }

                // Read from PTY (blocking)
                let n = {
                    let mut r: MutexGuard<'_, Box<dyn Read + Send>> = match reader.lock() {
                        Ok(r) => r,
                        Err(_) => {
                            tracing::error!("Read pump: Failed to acquire reader lock");
                            break;
                        }
                    };
                    match r.read(&mut buf) {
                        Ok(0) => {
                            tracing::debug!("Read pump: PTY EOF");
                            break;
                        }
                        Ok(n) => n,
                        Err(e) => {
                            // Check if it's a "would block" or interrupted error
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                || e.kind() == std::io::ErrorKind::Interrupted
                            {
                                continue;
                            }
                            tracing::error!("Read pump error: {}", e);
                            break;
                        }
                    }
                };

                // Combine remainder with new data for UTF-8 safe processing
                let mut to_send = if remainder.is_empty() {
                    buf[..n].to_vec()
                } else {
                    let mut combined = std::mem::take(&mut remainder);
                    combined.extend_from_slice(&buf[..n]);
                    combined
                };

                // Find UTF-8 safe boundary to avoid splitting multi-byte characters
                let safe_end = find_utf8_safe_boundary(&to_send);
                if safe_end < to_send.len() {
                    // Store incomplete UTF-8 sequence for next iteration
                    remainder = to_send[safe_end..].to_vec();
                    to_send.truncate(safe_end);
                }

                // Only send if we have data (might be empty if all bytes are partial UTF-8)
                if !to_send.is_empty() {
                    if let Err(e) = rt.block_on(event_tx.send(SessionEvent::Data(to_send))) {
                        tracing::error!("Failed to send data event: {}", e);
                        break;
                    }
                }
            }

            // Flush any remaining data before closing
            if !remainder.is_empty() {
                let _ = rt.block_on(event_tx.send(SessionEvent::Data(remainder)));
            }

            // Notify session closed
            running_read.store(false, Ordering::SeqCst);
            let _ = rt.block_on(event_tx.send(SessionEvent::Closed(None)));
            tracing::info!("Local terminal session {} read pump exited", session_id);
        });

        self._data_pump_handle = Some(handle);

        tracing::info!(
            "Local terminal session {} started with shell: {}",
            self.id,
            self.shell.label
        );
        Ok(())
    }

    /// Write data to the session (input from frontend)
    pub async fn write(&self, data: &[u8]) -> Result<(), SessionError> {
        if !self.running.load(Ordering::SeqCst) {
            return Err(SessionError::AlreadyClosed);
        }

        if let Some(tx) = &self.input_tx {
            tx.send(data.to_vec())
                .await
                .map_err(|_| SessionError::ChannelError)?;
        }
        Ok(())
    }

    /// Resize the terminal
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<(), SessionError> {
        self.cols = cols;
        self.rows = rows;

        if let Some(pty) = &self.pty {
            pty.resize(cols, rows)?;
        }
        Ok(())
    }

    /// Check if the session is running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Get session info for serialization
    pub fn info(&self) -> LocalTerminalInfo {
        LocalTerminalInfo {
            id: self.id.clone(),
            shell: self.shell.clone(),
            cols: self.cols,
            rows: self.rows,
            running: self.is_running(),
        }
    }

    /// Close the session
    pub fn close(&mut self) {
        tracing::info!("Closing local terminal session {}", self.id);
        self.running.store(false, Ordering::SeqCst);

        // Kill the entire PTY process group
        // This ensures all child processes (vim, btop, etc.) are cleaned up
        if let Some(pty) = self.pty.take() {
            let _ = pty.kill_process_group();
        }

        // Drop input channel
        self.input_tx = None;
    }
}

impl Drop for LocalTerminalSession {
    fn drop(&mut self) {
        self.close();
    }
}

/// Serializable session info for frontend
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LocalTerminalInfo {
    pub id: String,
    pub shell: ShellInfo,
    pub cols: u16,
    pub rows: u16,
    pub running: bool,
}

/// Find a safe UTF-8 boundary in a byte slice.
/// Returns the index up to which the bytes form valid, complete UTF-8 characters.
/// Any trailing incomplete multi-byte sequence is excluded.
///
/// This prevents splitting multi-byte characters (like CJK, emoji) across chunks,
/// which would cause xterm.js to render replacement characters (�).
fn find_utf8_safe_boundary(buf: &[u8]) -> usize {
    if buf.is_empty() {
        return 0;
    }

    // Start from the end and look for a safe boundary
    let mut i = buf.len();

    // Check the last few bytes (max UTF-8 char is 4 bytes)
    while i > 0 && i > buf.len().saturating_sub(4) {
        let byte = buf[i - 1];

        // ASCII byte (0xxxxxxx) - always a complete character
        if byte & 0x80 == 0 {
            return i;
        }

        // Continuation byte (10xxxxxx) - part of multi-byte sequence, keep going back
        if byte & 0xC0 == 0x80 {
            i -= 1;
            continue;
        }

        // Start of multi-byte sequence (11xxxxxx)
        // Determine expected length and check if it's complete
        let char_len = if byte & 0xF8 == 0xF0 {
            4 // 11110xxx - 4-byte sequence
        } else if byte & 0xF0 == 0xE0 {
            3 // 1110xxxx - 3-byte sequence
        } else if byte & 0xE0 == 0xC0 {
            2 // 110xxxxx - 2-byte sequence
        } else {
            // Invalid UTF-8 start byte, treat as boundary
            return i;
        };

        let start_pos = i - 1;
        let available = buf.len() - start_pos;

        if available >= char_len {
            // Complete character, include it
            return start_pos + char_len;
        } else {
            // Incomplete character, exclude it (return position before start byte)
            return start_pos;
        }
    }

    // If we've gone through all bytes without finding a safe boundary,
    // return the full length (might be all ASCII or we're at the start)
    buf.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new() {
        let shell = crate::local::shell::default_shell();
        let session = LocalTerminalSession::new(shell.clone(), 80, 24);

        assert_eq!(session.cols, 80);
        assert_eq!(session.rows, 24);
        assert_eq!(session.shell.id, shell.id);
        assert!(!session.is_running());
    }

    #[test]
    fn test_utf8_safe_boundary_ascii() {
        let data = b"hello world";
        assert_eq!(find_utf8_safe_boundary(data), 11);
    }

    #[test]
    fn test_utf8_safe_boundary_complete_cjk() {
        // "你好" in UTF-8: E4 BD A0 E5 A5 BD
        let data = "你好".as_bytes();
        assert_eq!(find_utf8_safe_boundary(data), 6);
    }

    #[test]
    fn test_utf8_safe_boundary_incomplete_cjk() {
        // "你" (E4 BD A0) with incomplete second char (E5 A5)
        let data: &[u8] = &[0xE4, 0xBD, 0xA0, 0xE5, 0xA5];
        assert_eq!(find_utf8_safe_boundary(data), 3); // Only complete "你"
    }

    #[test]
    fn test_utf8_safe_boundary_emoji() {
        // "😀" is F0 9F 98 80 (4 bytes)
        let data = "😀".as_bytes();
        assert_eq!(find_utf8_safe_boundary(data), 4);

        // Incomplete emoji (missing last byte)
        let incomplete: &[u8] = &[0xF0, 0x9F, 0x98];
        assert_eq!(find_utf8_safe_boundary(incomplete), 0);
    }
}
