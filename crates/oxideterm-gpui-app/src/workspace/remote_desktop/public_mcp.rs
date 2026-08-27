// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_public_mcp::{
    DesktopButtonState, DesktopClipboardKind, DesktopInputEvent, PublicDesktopMouseButton,
};

pub(in crate::workspace) enum RemoteDesktopPublicClipboardSnapshot {
    Text(Zeroizing<String>),
    Image {
        format: RemoteDesktopClipboardFormat,
        bytes: Zeroizing<Vec<u8>>,
    },
}

impl RemoteDesktopSessionEntity {
    pub(in crate::workspace) fn public_mcp_state_projection(&self) -> serde_json::Value {
        let snapshot = self.state.snapshot();
        let clipboard = self.profile.session_options.clipboard;
        let image_clipboard_available = self.public_mcp_image_clipboard_available(&snapshot);
        serde_json::json!({
            "protocol": snapshot.protocol,
            "status": snapshot.status,
            "size": snapshot.size,
            "error_category": snapshot.error_category,
            "read_only": snapshot.read_only,
            "has_frame": snapshot.has_frame,
            "graphics_epoch": snapshot.graphics_epoch,
            "frame_generation": snapshot.frame_generation,
            "pending_resize": snapshot.pending_resize,
            "provider_capabilities": self.provider.capabilities,
            "negotiated_capabilities": snapshot.negotiated_capabilities,
            // Reconnect is exposed by the logical Active Sessions record, not this tab handle.
            "can_reconnect": false,
            "clipboard": {
                "read_text": clipboard.text && self.provider.capabilities.clipboard_text,
                "write_text": !snapshot.read_only && clipboard.text && self.provider.capabilities.clipboard_text,
                "read_image": image_clipboard_available,
                "write_image": !snapshot.read_only && image_clipboard_available,
            }
        })
    }

    pub(in crate::workspace) fn public_mcp_frame_snapshot(
        &self,
    ) -> Option<oxideterm_gpui_remote_desktop::RemoteDesktopFrameSnapshot> {
        self.state.frame_snapshot()
    }

    pub(in crate::workspace) fn attach_public_mcp_frame_observer(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.public_mcp_frame_observers = self.public_mcp_frame_observers.saturating_add(1);
        self.apply_frame_visibility(cx);
    }

    pub(in crate::workspace) fn detach_public_mcp_frame_observer(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.public_mcp_frame_observers = self.public_mcp_frame_observers.saturating_sub(1);
        self.apply_frame_visibility(cx);
    }

    pub(in crate::workspace) fn release_public_mcp_inputs(&mut self) {
        // Revoking input authority releases every edge even if the client
        // disappeared between a press and its matching release.
        self.release_inputs();
    }

    pub(in crate::workspace) fn apply_public_mcp_input(
        &mut self,
        graphics_epoch: u64,
        event: &DesktopInputEvent,
    ) -> Result<(), String> {
        if matches!(event, DesktopInputEvent::ReleaseAll) {
            self.release_inputs();
            return Ok(());
        }
        let snapshot = self.state.snapshot();
        if snapshot.status != RemoteDesktopSessionStatus::Connected {
            return Err("The remote desktop session is not connected".to_owned());
        }
        if snapshot.read_only {
            return Err("The remote desktop session is read-only".to_owned());
        }
        if snapshot.graphics_epoch != Some(graphics_epoch) {
            return Err("The framebuffer epoch is stale".to_owned());
        }
        let size = snapshot
            .size
            .ok_or_else(|| "The framebuffer size is unavailable".to_owned())?;
        match event {
            DesktopInputEvent::MouseMove { x, y } => {
                require_public_mcp_frame_point(size, *x, *y)?;
                self.state.apply_event(RemoteDesktopHelperEvent::Cursor {
                    x: *x,
                    y: *y,
                    width: 0,
                    height: 0,
                });
                self.send_request(RemoteDesktopHelperRequest::MouseMove { x: *x, y: *y });
            }
            DesktopInputEvent::MouseButton {
                x,
                y,
                button,
                state,
            } => {
                require_public_mcp_frame_point(size, *x, *y)?;
                let button = public_mcp_mouse_button(*button);
                let state = public_mcp_button_state(*state);
                match state {
                    RemoteDesktopMouseButtonState::Pressed => {
                        self.pressed_mouse_buttons.insert(button);
                    }
                    RemoteDesktopMouseButtonState::Released => {
                        self.pressed_mouse_buttons.remove(&button);
                    }
                }
                // Pointer edges are atomic at the public boundary: every button event
                // first establishes its exact framebuffer coordinate.
                self.send_request(RemoteDesktopHelperRequest::MouseMove { x: *x, y: *y });
                self.send_request(RemoteDesktopHelperRequest::MouseButton { button, state });
            }
            DesktopInputEvent::Wheel {
                x,
                y,
                delta_x,
                delta_y,
            } => {
                require_public_mcp_frame_point(size, *x, *y)?;
                self.send_request(RemoteDesktopHelperRequest::MouseMove { x: *x, y: *y });
                self.send_request(RemoteDesktopHelperRequest::Wheel {
                    delta: RemoteDesktopWheelDelta {
                        x: *delta_x,
                        y: *delta_y,
                    },
                });
            }
            DesktopInputEvent::Key {
                code,
                text,
                alt,
                ctrl,
                shift,
                meta,
                state,
            } => {
                self.send_request(RemoteDesktopHelperRequest::Key {
                    key: RemoteDesktopKey {
                        code: code.clone(),
                        text: text.as_deref().map(ToOwned::to_owned),
                        alt: *alt,
                        ctrl: *ctrl,
                        shift: *shift,
                        meta: *meta,
                    },
                    state: match state {
                        DesktopButtonState::Pressed => RemoteDesktopKeyState::Pressed,
                        DesktopButtonState::Released => RemoteDesktopKeyState::Released,
                    },
                });
            }
            DesktopInputEvent::Text { text } => {
                self.send_request(RemoteDesktopHelperRequest::Text {
                    text: text.to_string(),
                });
            }
            DesktopInputEvent::ReleaseAll => unreachable!("release-all returns before validation"),
        }
        Ok(())
    }

    pub(in crate::workspace) fn apply_public_mcp_resize(
        &mut self,
        size: RemoteDesktopSize,
    ) -> Result<(), String> {
        if !self.provider.capabilities.resize {
            return Err("The remote desktop provider does not support resize".to_owned());
        }
        if self.state.snapshot().status != RemoteDesktopSessionStatus::Connected {
            return Err("The remote desktop session is not connected".to_owned());
        }
        self.send_request(RemoteDesktopHelperRequest::Resize {
            size,
            scale_factor: None,
        });
        Ok(())
    }

    pub(in crate::workspace) fn public_mcp_clipboard_snapshot(
        &self,
        kind: DesktopClipboardKind,
    ) -> Result<RemoteDesktopPublicClipboardSnapshot, String> {
        match (kind, self.public_mcp_clipboard.as_ref()) {
            (DesktopClipboardKind::Text, Some(RemoteDesktopPublicClipboard::Text(text))) => Ok(
                RemoteDesktopPublicClipboardSnapshot::Text(Zeroizing::new(text.to_string())),
            ),
            (
                DesktopClipboardKind::Image,
                Some(RemoteDesktopPublicClipboard::Image { format, bytes }),
            ) => Ok(RemoteDesktopPublicClipboardSnapshot::Image {
                format: *format,
                bytes: Zeroizing::new(bytes.to_vec()),
            }),
            _ => Err("The requested remote clipboard value is unavailable".to_owned()),
        }
    }

    pub(in crate::workspace) fn clear_public_mcp_clipboard(&mut self) {
        // The platform clipboard remains owned by the desktop UI; this only revokes MCP content.
        self.public_mcp_clipboard = None;
    }

    pub(in crate::workspace) fn write_public_mcp_clipboard_text(
        &mut self,
        text: &str,
    ) -> Result<(), String> {
        self.require_public_mcp_clipboard_write(false)?;
        self.send_request(RemoteDesktopHelperRequest::ClipboardText {
            text: text.to_owned(),
        });
        Ok(())
    }

    pub(in crate::workspace) fn write_public_mcp_clipboard_image(
        &mut self,
        format: RemoteDesktopClipboardFormat,
        bytes: &[u8],
    ) -> Result<(), String> {
        self.require_public_mcp_clipboard_write(true)?;
        self.send_request(RemoteDesktopHelperRequest::ClipboardData {
            data: RemoteDesktopClipboardData::new(format, bytes.to_vec()),
        });
        Ok(())
    }

    fn require_public_mcp_clipboard_write(&self, image: bool) -> Result<(), String> {
        let snapshot = self.state.snapshot();
        if snapshot.status != RemoteDesktopSessionStatus::Connected {
            return Err("The remote desktop session is not connected".to_owned());
        }
        if snapshot.read_only {
            return Err("The remote desktop session is read-only".to_owned());
        }
        let enabled = if image {
            self.public_mcp_image_clipboard_available(&snapshot)
        } else {
            self.provider.capabilities.clipboard_text && self.profile.session_options.clipboard.text
        };
        enabled
            .then_some(())
            .ok_or_else(|| "The requested clipboard direction is disabled".to_owned())
    }

    fn public_mcp_image_clipboard_available(
        &self,
        snapshot: &oxideterm_gpui_remote_desktop::RemoteDesktopViewSnapshot,
    ) -> bool {
        self.provider.capabilities.clipboard_data
            && self.profile.session_options.clipboard.images
            && (self.profile.protocol != RemoteDesktopProtocol::Vnc
                || snapshot
                    .negotiated_capabilities
                    .as_ref()
                    .is_some_and(|capabilities| {
                        capabilities.extended_clipboard == NegotiatedCapabilityStatus::Supported
                            && capabilities
                                .extended_clipboard_formats
                                .iter()
                                .any(|format| format == "dib-v5")
                    }))
    }
}

fn require_public_mcp_frame_point(size: RemoteDesktopSize, x: u32, y: u32) -> Result<(), String> {
    (x < size.width && y < size.height)
        .then_some(())
        .ok_or_else(|| "The input coordinate is outside the current framebuffer".to_owned())
}

fn public_mcp_mouse_button(button: PublicDesktopMouseButton) -> RemoteDesktopMouseButton {
    match button {
        PublicDesktopMouseButton::Left => RemoteDesktopMouseButton::Left,
        PublicDesktopMouseButton::Middle => RemoteDesktopMouseButton::Middle,
        PublicDesktopMouseButton::Right => RemoteDesktopMouseButton::Right,
        PublicDesktopMouseButton::Back => RemoteDesktopMouseButton::Back,
        PublicDesktopMouseButton::Forward => RemoteDesktopMouseButton::Forward,
    }
}

fn public_mcp_button_state(state: DesktopButtonState) -> RemoteDesktopMouseButtonState {
    match state {
        DesktopButtonState::Pressed => RemoteDesktopMouseButtonState::Pressed,
        DesktopButtonState::Released => RemoteDesktopMouseButtonState::Released,
    }
}
