// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_remote_desktop::{
    RemoteDesktopCursorShape, RemoteDesktopFrame, RemoteDesktopHelperEvent, RemoteDesktopProtocol,
    RemoteDesktopSessionStatus, RemoteDesktopSize,
};

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteDesktopCursorState {
    pub x: u32,
    pub y: u32,
    pub visible: bool,
    pub shape: Option<RemoteDesktopCursorShape>,
}

impl Default for RemoteDesktopCursorState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            visible: true,
            shape: None,
        }
    }
}

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
    cursor: RemoteDesktopCursorState,
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
            cursor: RemoteDesktopCursorState::default(),
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
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                if let Some(frame) = self.frame.as_mut()
                    && frame.apply_update(&update)
                {
                    self.status = RemoteDesktopSessionStatus::Connected;
                    self.size = Some(update.size);
                    self.message = None;
                    self.pending_resize = None;
                }
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
                if self.status == RemoteDesktopSessionStatus::Disconnected && self.message.is_some()
                {
                    return;
                }
                self.status = RemoteDesktopSessionStatus::Disconnected;
                self.message = exit_code.map(|code| format!("Helper exited with code {code}."));
                self.frame = None;
            }
            RemoteDesktopHelperEvent::Cursor { x, y, .. } => {
                self.cursor.x = x;
                self.cursor.y = y;
            }
            RemoteDesktopHelperEvent::CursorShape { shape } => {
                if shape.is_complete() {
                    self.cursor.shape = Some(shape);
                    self.cursor.visible = true;
                }
            }
            RemoteDesktopHelperEvent::CursorDefault => {
                self.cursor.shape = None;
                self.cursor.visible = true;
            }
            RemoteDesktopHelperEvent::CursorHidden => {
                self.cursor.visible = false;
            }
            RemoteDesktopHelperEvent::ClipboardText { .. } => {
                // Clipboard changes are handled by the app surface that owns
                // focus, clipboard, and pointer capture.
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

    pub fn cursor(&self) -> &RemoteDesktopCursorState {
        &self.cursor
    }
}

#[cfg(test)]
mod tests {
    use oxideterm_remote_desktop::{
        RemoteDesktopCursorShape, RemoteDesktopFrame, RemoteDesktopFrameFormat,
        RemoteDesktopFrameUpdate, RemoteDesktopRect,
    };

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
    fn frame_update_patches_existing_frame() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 2,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![1, 1, 1, 1, 2, 2, 2, 2],
            ),
        });

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(1, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![9, 9, 9, 9],
            ),
        });

        assert_eq!(state.frame().unwrap().bytes, vec![1, 1, 1, 1, 9, 9, 9, 9]);
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

    #[test]
    fn terminated_event_does_not_hide_disconnect_reason() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::Disconnected {
            reason: Some("RDP session closed.".to_string()),
        });
        state.apply_event(RemoteDesktopHelperEvent::Terminated { exit_code: Some(0) });

        let snapshot = state.snapshot();
        assert_eq!(snapshot.status, RemoteDesktopSessionStatus::Disconnected);
        assert_eq!(snapshot.message.as_deref(), Some("RDP session closed."));
    }

    #[test]
    fn cursor_events_update_remote_cursor_state() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let shape = RemoteDesktopCursorShape::new(
            RemoteDesktopSize {
                width: 1,
                height: 1,
            },
            0,
            0,
            RemoteDesktopFrameFormat::Rgba8,
            vec![1, 2, 3, 4],
        );

        state.apply_event(RemoteDesktopHelperEvent::CursorShape {
            shape: shape.clone(),
        });
        state.apply_event(RemoteDesktopHelperEvent::Cursor {
            x: 12,
            y: 34,
            width: 1,
            height: 1,
        });

        assert_eq!(state.cursor().x, 12);
        assert_eq!(state.cursor().y, 34);
        assert!(state.cursor().visible);
        assert_eq!(state.cursor().shape.as_ref(), Some(&shape));

        state.apply_event(RemoteDesktopHelperEvent::CursorHidden);
        assert!(!state.cursor().visible);

        state.apply_event(RemoteDesktopHelperEvent::CursorDefault);
        assert!(state.cursor().visible);
        assert!(state.cursor().shape.is_none());
    }
}
