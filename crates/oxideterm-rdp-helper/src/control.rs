// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn rdp_bitmap_codec_label(codec_id: u8) -> &'static str {
    match CodecId::from_u8(codec_id) {
        Some(id) if id == CODEC_ID_NONE => "none",
        Some(id) if id == CODEC_ID_REMOTEFX => "remotefx",
        Some(id) if id == CODEC_ID_QOI => "qoi",
        Some(id) if id == CODEC_ID_QOIZ => "qoiz",
        _ => "unknown",
    }
}

pub(super) fn forward_client_rdp_request(
    input_tx: &tokio_mpsc::UnboundedSender<RdpInputEvent>,
    input_database: &mut RdpInputDatabase,
    keyboard_mapper: &mut RdpKeyboardInputMapper,
    request: RemoteDesktopHelperRequest,
    read_only: bool,
) -> Result<(), String> {
    match request {
        RemoteDesktopHelperRequest::Authenticate {
            challenge_id,
            sha256_fingerprint,
            username,
            password,
            domain,
        } => {
            input_tx
                .send(RdpInputEvent::Authenticate {
                    challenge_id,
                    sha256_fingerprint,
                    username,
                    password,
                    domain,
                })
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::Resize { size, scale_factor } => {
            let requested_size = normalized_rdp_desktop_size(size);
            input_tx
                .send(RdpInputEvent::Resize {
                    width: clamp_u32_to_u16(requested_size.width),
                    height: clamp_u32_to_u16(requested_size.height),
                    scale_factor: rdp_displaycontrol_scale_factor(scale_factor),
                    physical_size: None,
                })
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::MouseMove { x, y } if !read_only => {
            send_client_rdp_input_operations(
                input_tx,
                input_database,
                [RdpInputOperation::MouseMove(MousePosition {
                    x: clamp_u32_to_u16(x),
                    y: clamp_u32_to_u16(y),
                })],
            )?;
        }
        RemoteDesktopHelperRequest::MouseButton { button, state } if !read_only => {
            if let Some(button) = rdp_mouse_button(button) {
                let operation = match state {
                    RemoteDesktopMouseButtonState::Pressed => {
                        RdpInputOperation::MouseButtonPressed(button)
                    }
                    RemoteDesktopMouseButtonState::Released => {
                        RdpInputOperation::MouseButtonReleased(button)
                    }
                };
                send_client_rdp_input_operations(input_tx, input_database, [operation])?;
            }
        }
        RemoteDesktopHelperRequest::Wheel { delta } if !read_only => {
            send_client_rdp_input_operations(
                input_tx,
                input_database,
                rdp_wheel_operations(delta),
            )?;
        }
        RemoteDesktopHelperRequest::Key { key, state } if !read_only => {
            send_client_rdp_input_operations(
                input_tx,
                input_database,
                keyboard_mapper.operations(&key, state),
            )?;
        }
        RemoteDesktopHelperRequest::Text { text } if !read_only => {
            for character in text.chars().filter(|character| !character.is_control()) {
                send_client_rdp_input_operations(
                    input_tx,
                    input_database,
                    [
                        RdpInputOperation::UnicodeKeyPressed(character),
                        RdpInputOperation::UnicodeKeyReleased(character),
                    ],
                )?;
            }
        }
        RemoteDesktopHelperRequest::ClipboardText { text } if !read_only => {
            input_tx
                .send(RdpInputEvent::SetClipboardText(text))
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::ClipboardData { data } if !read_only => {
            input_tx
                .send(RdpInputEvent::SetClipboardData(data))
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::ClipboardFiles { transfer_id, paths } if !read_only => {
            input_tx
                .send(RdpInputEvent::SetClipboardFiles { transfer_id, paths })
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::UpdateDisplayLayout { layout } => {
            input_tx
                .send(RdpInputEvent::UpdateDisplayLayout(layout))
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::CancelClipboardTransfer { transfer_id } => {
            input_tx
                .send(RdpInputEvent::CancelClipboardTransfer(transfer_id))
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::SynchronizeLockKeys { keys } if !read_only => {
            send_client_rdp_lock_key_state(input_tx, keys)?;
        }
        RemoteDesktopHelperRequest::RequestFrame => {
            input_tx
                .send(RdpInputEvent::RequestFrame)
                .map_err(|_| "RDP input channel is closed.".to_string())?;
        }
        RemoteDesktopHelperRequest::ReleaseAllInputs if !read_only => {
            // Release mapper-owned Unicode and synthetic modifier state before
            // asking IronRDP's database to release the protocol-owned state.
            send_client_rdp_input_operations(
                input_tx,
                input_database,
                keyboard_mapper.release_all_operations(),
            )?;
            let events = input_database.release_all();
            if !events.is_empty() {
                input_tx
                    .send(RdpInputEvent::FastPath(events))
                    .map_err(|_| "RDP input channel is closed.".to_string())?;
            }
        }
        RemoteDesktopHelperRequest::StartConnect { .. }
        | RemoteDesktopHelperRequest::Connect { .. }
        | RemoteDesktopHelperRequest::Close
        | RemoteDesktopHelperRequest::Reconnect
        | RemoteDesktopHelperRequest::MouseMove { .. }
        | RemoteDesktopHelperRequest::MouseButton { .. }
        | RemoteDesktopHelperRequest::Wheel { .. }
        | RemoteDesktopHelperRequest::Key { .. }
        | RemoteDesktopHelperRequest::Text { .. }
        | RemoteDesktopHelperRequest::ClipboardText { .. }
        | RemoteDesktopHelperRequest::ClipboardData { .. }
        | RemoteDesktopHelperRequest::ClipboardFiles { .. }
        | RemoteDesktopHelperRequest::SynchronizeLockKeys { .. }
        | RemoteDesktopHelperRequest::ReleaseAllInputs => {}
    }
    Ok(())
}

pub(super) fn send_client_rdp_lock_key_state(
    input_tx: &tokio_mpsc::UnboundedSender<RdpInputEvent>,
    keys: RemoteDesktopLockKeys,
) -> Result<(), String> {
    let mut events = SmallVec::new();
    // IronRDP owns the exact fast-path synchronize flag mapping. Keep this
    // helper as a transport boundary instead of duplicating the protocol bits.
    events.push(rdp_synchronize_event(
        keys.scroll_lock,
        keys.num_lock,
        keys.caps_lock,
        keys.kana_lock,
    ));
    input_tx
        .send(RdpInputEvent::FastPath(events))
        .map_err(|_| "RDP input channel is closed.".to_string())
}

pub(super) fn send_client_rdp_input_operations<I>(
    input_tx: &tokio_mpsc::UnboundedSender<RdpInputEvent>,
    input_database: &mut RdpInputDatabase,
    operations: I,
) -> Result<(), String>
where
    I: IntoIterator<Item = RdpInputOperation>,
{
    let events = input_database.apply(operations);
    if events.is_empty() {
        return Ok(());
    }
    input_tx
        .send(RdpInputEvent::FastPath(events))
        .map_err(|_| "RDP input channel is closed.".to_string())
}
