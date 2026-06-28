// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopHelperEvent, RemoteDesktopProtocol,
    RemoteDesktopSessionStatus, RemoteDesktopSize,
};

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteDesktopViewSnapshot {
    pub title: String,
    pub protocol: RemoteDesktopProtocol,
    pub status: RemoteDesktopSessionStatus,
    pub size: Option<RemoteDesktopSize>,
    pub message: Option<String>,
    pub has_frame: bool,
    pub read_only: bool,
    pub pending_resize: Option<RemoteDesktopSize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteDesktopViewState {
    title: String,
    protocol: RemoteDesktopProtocol,
    status: RemoteDesktopSessionStatus,
    size: Option<RemoteDesktopSize>,
    message: Option<String>,
    frame: Option<RemoteDesktopFrame>,
    read_only: bool,
    pending_resize: Option<RemoteDesktopSize>,
}

impl RemoteDesktopViewState {
    pub fn new(title: impl Into<String>, protocol: RemoteDesktopProtocol) -> Self {
        Self {
            title: title.into(),
            protocol,
            status: RemoteDesktopSessionStatus::Idle,
            size: None,
            message: None,
            frame: None,
            read_only: false,
            pending_resize: None,
        }
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn apply_event(&mut self, event: RemoteDesktopHelperEvent) {
        match event {
            RemoteDesktopHelperEvent::Status { status, message } => {
                self.status = status;
                self.message = message;
            }
            RemoteDesktopHelperEvent::Connected { size } => {
                self.status = RemoteDesktopSessionStatus::Connected;
                self.size = Some(size);
                self.message = None;
                self.pending_resize = None;
            }
            RemoteDesktopHelperEvent::Frame { frame } => {
                self.status = RemoteDesktopSessionStatus::Connected;
                self.size = Some(frame.size);
                self.frame = Some(frame);
                self.message = None;
                self.pending_resize = None;
            }
            RemoteDesktopHelperEvent::ConnectionFailure { message } => {
                self.status = RemoteDesktopSessionStatus::Failed;
                self.message = Some(message);
            }
            RemoteDesktopHelperEvent::Disconnected { reason } => {
                self.status = RemoteDesktopSessionStatus::Disconnected;
                self.message = reason;
                self.frame = None;
            }
            RemoteDesktopHelperEvent::Terminated { exit_code } => {
                self.status = RemoteDesktopSessionStatus::Disconnected;
                self.message = exit_code.map(|code| format!("Helper exited with code {code}."));
                self.frame = None;
            }
            RemoteDesktopHelperEvent::Cursor { .. }
            | RemoteDesktopHelperEvent::ClipboardText { .. } => {
                // Cursor and clipboard changes are handled by the app surface
                // that owns focus, clipboard, and pointer capture.
            }
        }
    }

    pub fn mark_resize_requested(&mut self, size: RemoteDesktopSize) {
        self.pending_resize = Some(RemoteDesktopSize::clamped(size.width, size.height));
    }

    pub fn snapshot(&self) -> RemoteDesktopViewSnapshot {
        RemoteDesktopViewSnapshot {
            title: self.title.clone(),
            protocol: self.protocol,
            status: self.status,
            size: self.size,
            message: self.message.clone(),
            has_frame: self.frame.is_some(),
            read_only: self.read_only,
            pending_resize: self.pending_resize,
        }
    }

    pub fn frame(&self) -> Option<&RemoteDesktopFrame> {
        self.frame.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use oxideterm_remote_desktop::{RemoteDesktopFrame, RemoteDesktopFrameFormat};

    use super::*;

    #[test]
    fn connected_event_sets_size_and_status() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::Connected {
            size: RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
        });

        let snapshot = state.snapshot();
        assert_eq!(snapshot.status, RemoteDesktopSessionStatus::Connected);
        assert_eq!(
            snapshot.size,
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            })
        );
        assert!(!snapshot.has_frame);
    }

    #[test]
    fn frame_event_keeps_latest_frame_for_rendering() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Vnc);
        let size = RemoteDesktopSize {
            width: 2,
            height: 2,
        };

        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(size, RemoteDesktopFrameFormat::Rgba8, vec![0; 16]),
        });

        assert!(state.snapshot().has_frame);
        assert!(state.frame().unwrap().is_complete());
    }

    #[test]
    fn connected_event_clears_pending_resize() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Vnc);
        state.mark_resize_requested(RemoteDesktopSize {
            width: 1200,
            height: 900,
        });

        state.apply_event(RemoteDesktopHelperEvent::Connected {
            size: RemoteDesktopSize {
                width: 1200,
                height: 900,
            },
        });

        assert_eq!(state.snapshot().pending_resize, None);
    }

    #[test]
    fn failure_event_exposes_user_safe_message() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::ConnectionFailure {
            message: "authentication failed".to_string(),
        });

        let snapshot = state.snapshot();
        assert_eq!(snapshot.status, RemoteDesktopSessionStatus::Failed);
        assert_eq!(snapshot.message.as_deref(), Some("authentication failed"));
    }
}
