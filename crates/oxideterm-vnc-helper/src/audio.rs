// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io::Read;

use oxideterm_pcm_audio::PcmS16LePlayback;

use super::*;

const QEMU_AUDIO_MESSAGE_TYPE: u8 = 255;
const QEMU_AUDIO_SUBTYPE: u8 = 1;
const QEMU_AUDIO_CLIENT_ENABLE: u16 = 0;
const QEMU_AUDIO_CLIENT_DISABLE: u16 = 1;
const QEMU_AUDIO_CLIENT_SET_FORMAT: u16 = 2;
const QEMU_AUDIO_SERVER_STOP: u16 = 0;
const QEMU_AUDIO_SERVER_START: u16 = 1;
const QEMU_AUDIO_SERVER_DATA: u16 = 2;
const QEMU_AUDIO_FORMAT_S16: u8 = 3;
const QEMU_AUDIO_CHANNELS: u8 = 2;
const QEMU_AUDIO_SAMPLE_RATE: u32 = 44_100;
const QEMU_AUDIO_BYTES_PER_FRAME: usize = QEMU_AUDIO_CHANNELS as usize * size_of::<i16>();
const MAX_QEMU_AUDIO_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Represents one QEMU Audio server message after its outer message type.
#[derive(Debug, PartialEq)]
pub(super) enum QemuAudioServerMessage {
    Stop,
    Start,
    Data(Vec<u8>),
}

/// Owns QEMU Audio negotiation state and the joinable local playback worker.
#[derive(Debug)]
pub(super) struct QemuAudioSession {
    requested: bool,
    server_supported: bool,
    client_enabled: bool,
    server_streaming: bool,
    playback: PcmS16LePlayback,
}

impl QemuAudioSession {
    /// Creates a disabled session until the server confirms the pseudo-encoding.
    pub(super) fn new(requested: bool) -> Self {
        Self {
            requested,
            server_supported: false,
            client_enabled: false,
            server_streaming: false,
            playback: PcmS16LePlayback::new(QEMU_AUDIO_SAMPLE_RATE, u16::from(QEMU_AUDIO_CHANNELS)),
        }
    }

    /// Records server support and enables the negotiated playback format once.
    pub(super) fn confirm_server_support(
        &mut self,
        writer: &SharedVncWriter,
    ) -> Result<bool, String> {
        let newly_supported = !self.server_supported;
        self.server_supported = true;
        if self.requested && !self.client_enabled {
            write_vnc_message(writer, &qemu_audio_set_format_message())?;
            write_vnc_message(
                writer,
                &qemu_audio_control_message(QEMU_AUDIO_CLIENT_ENABLE),
            )?;
            self.client_enabled = true;
        }
        Ok(newly_supported)
    }

    /// Applies server audio only after both user opt-in and server confirmation.
    pub(super) fn handle_server_message(&mut self, message: &QemuAudioServerMessage) {
        if !self.requested || !self.server_supported || !self.client_enabled {
            return;
        }
        match message {
            QemuAudioServerMessage::Start => {
                self.server_streaming = match self.playback.start() {
                    Ok(()) => true,
                    Err(error) => {
                        eprintln!("[oxideterm:vnc-audio] playback unavailable: {error}");
                        false
                    }
                };
            }
            QemuAudioServerMessage::Data(bytes) if self.server_streaming => {
                self.playback.push(bytes);
            }
            QemuAudioServerMessage::Stop => {
                self.server_streaming = false;
                self.playback.stop();
            }
            QemuAudioServerMessage::Data(_) => {}
        }
    }

    /// Disables the server stream and joins local playback during session cleanup.
    pub(super) fn shutdown(&mut self, writer: &SharedVncWriter) {
        if self.client_enabled {
            let _ = write_vnc_message(
                writer,
                &qemu_audio_control_message(QEMU_AUDIO_CLIENT_DISABLE),
            );
        }
        self.client_enabled = false;
        self.server_streaming = false;
        self.playback.stop();
    }

    /// Stops local playback after transport loss when no wire write is possible.
    pub(super) fn stop_local(&mut self) {
        self.client_enabled = false;
        self.server_streaming = false;
        self.playback.stop();
    }
}

/// Applies audio events before framebuffer consumption and reports new support.
pub(super) fn handle_qemu_audio_event(
    event: &VncServerEvent,
    audio: &Arc<Mutex<QemuAudioSession>>,
    writer: &SharedVncWriter,
) -> Result<bool, String> {
    match event {
        VncServerEvent::QemuAudioCapability => audio
            .lock()
            .map_err(|_| "VNC QEMU Audio state lock is poisoned.".to_string())?
            .confirm_server_support(writer),
        VncServerEvent::QemuAudio(message) => {
            audio
                .lock()
                .map_err(|_| "VNC QEMU Audio state lock is poisoned.".to_string())?
                .handle_server_message(message);
            Ok(false)
        }
        VncServerEvent::Batch(events) => {
            let mut newly_supported = false;
            for event in events {
                newly_supported |= handle_qemu_audio_event(event, audio, writer)?;
            }
            Ok(newly_supported)
        }
        _ => Ok(false),
    }
}

impl Drop for QemuAudioSession {
    fn drop(&mut self) {
        // The playback backend owns and joins its device thread on drop.
        self.playback.stop();
    }
}

/// Parses QEMU's vendor audio payload after RFB message type 255.
pub(super) fn read_qemu_audio_server_message(
    reader: &mut impl Read,
) -> Result<QemuAudioServerMessage, String> {
    let subtype =
        read_u8(reader).map_err(|error| format!("VNC QEMU Audio subtype read failed: {error}"))?;
    if subtype != QEMU_AUDIO_SUBTYPE {
        return Err(format!("Unsupported VNC QEMU message subtype {subtype}."));
    }
    let operation = read_be_u16(reader)
        .map_err(|error| format!("VNC QEMU Audio operation read failed: {error}"))?;
    match operation {
        QEMU_AUDIO_SERVER_STOP => Ok(QemuAudioServerMessage::Stop),
        QEMU_AUDIO_SERVER_START => Ok(QemuAudioServerMessage::Start),
        QEMU_AUDIO_SERVER_DATA => {
            let payload_len = read_be_u32(reader)
                .map_err(|error| format!("VNC QEMU Audio length read failed: {error}"))?
                as usize;
            if payload_len > MAX_QEMU_AUDIO_PAYLOAD_BYTES {
                return Err(format!(
                    "VNC QEMU Audio payload exceeds {MAX_QEMU_AUDIO_PAYLOAD_BYTES} bytes."
                ));
            }
            if payload_len % QEMU_AUDIO_BYTES_PER_FRAME != 0 {
                return Err(format!(
                    "VNC QEMU Audio payload length {payload_len} splits a stereo PCM frame."
                ));
            }
            read_exact_vec(reader, payload_len)
                .map(QemuAudioServerMessage::Data)
                .map_err(|error| format!("VNC QEMU Audio payload read failed: {error}"))
        }
        _ => Err(format!("Unsupported VNC QEMU Audio operation {operation}.")),
    }
}

/// Builds the exact S16 stereo format message accepted by QEMU.
fn qemu_audio_set_format_message() -> Vec<u8> {
    let mut message = Vec::with_capacity(10);
    message.push(QEMU_AUDIO_MESSAGE_TYPE);
    message.push(QEMU_AUDIO_SUBTYPE);
    push_be_u16(&mut message, QEMU_AUDIO_CLIENT_SET_FORMAT);
    message.push(QEMU_AUDIO_FORMAT_S16);
    message.push(QEMU_AUDIO_CHANNELS);
    push_be_u32(&mut message, QEMU_AUDIO_SAMPLE_RATE);
    message
}

/// Builds an enable or disable QEMU Audio control message.
fn qemu_audio_control_message(operation: u16) -> Vec<u8> {
    let mut message = Vec::with_capacity(4);
    message.push(QEMU_AUDIO_MESSAGE_TYPE);
    message.push(QEMU_AUDIO_SUBTYPE);
    push_be_u16(&mut message, operation);
    message
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::mpsc::{Receiver, TryRecvError};

    use super::*;

    /// Extracts one queued wire write from the VNC I/O command channel.
    fn next_write(receiver: &Receiver<VncIoCommand>) -> Vec<u8> {
        match receiver.try_recv().unwrap() {
            VncIoCommand::Write(message) => message,
            VncIoCommand::Shutdown => panic!("expected VNC audio write"),
        }
    }

    #[test]
    fn client_messages_match_qemu_audio_wire_format() {
        assert_eq!(
            qemu_audio_set_format_message(),
            vec![255, 1, 0, 2, 3, 2, 0, 0, 172, 68]
        );
        assert_eq!(
            qemu_audio_control_message(QEMU_AUDIO_CLIENT_ENABLE),
            vec![255, 1, 0, 0]
        );
        assert_eq!(
            qemu_audio_control_message(QEMU_AUDIO_CLIENT_DISABLE),
            vec![255, 1, 0, 1]
        );
    }

    #[test]
    fn capability_confirmation_enables_audio_only_after_user_opt_in() {
        let (writer, receiver) = std::sync::mpsc::sync_channel(4);
        let mut enabled = QemuAudioSession::new(true);

        assert!(enabled.confirm_server_support(&writer).unwrap());
        assert_eq!(next_write(&receiver), qemu_audio_set_format_message());
        assert_eq!(
            next_write(&receiver),
            qemu_audio_control_message(QEMU_AUDIO_CLIENT_ENABLE)
        );
        assert!(!enabled.confirm_server_support(&writer).unwrap());
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));

        let (writer, receiver) = std::sync::mpsc::sync_channel(4);
        let mut disabled = QemuAudioSession::new(false);
        assert!(disabled.confirm_server_support(&writer).unwrap());
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn shutdown_sends_disable_for_an_enabled_server_stream() {
        let (writer, receiver) = std::sync::mpsc::sync_channel(2);
        let mut session = QemuAudioSession::new(true);
        session.server_supported = true;
        session.client_enabled = true;

        session.shutdown(&writer);

        assert_eq!(
            next_write(&receiver),
            qemu_audio_control_message(QEMU_AUDIO_CLIENT_DISABLE)
        );
        assert!(!session.client_enabled);
    }

    #[test]
    fn parser_reads_start_stop_and_data_messages() {
        assert_eq!(
            read_qemu_audio_server_message(&mut Cursor::new([1, 0, 1])).unwrap(),
            QemuAudioServerMessage::Start
        );
        assert_eq!(
            read_qemu_audio_server_message(&mut Cursor::new([1, 0, 0])).unwrap(),
            QemuAudioServerMessage::Stop
        );
        assert_eq!(
            read_qemu_audio_server_message(&mut Cursor::new([1, 0, 2, 0, 0, 0, 4, 1, 2, 3, 4,]))
                .unwrap(),
            QemuAudioServerMessage::Data(vec![1, 2, 3, 4])
        );
    }

    #[test]
    fn framebuffer_parser_requires_server_confirmation_for_audio_support() {
        let mut payload = vec![0, 0, 1];
        payload.extend_from_slice(&[0; 8]);
        payload.extend_from_slice(&VNC_ENCODING_QEMU_AUDIO.to_be_bytes());

        assert_eq!(
            read_framebuffer_update(&mut Cursor::new(payload), &mut VncDecodeState::default(),)
                .unwrap(),
            VncServerEvent::Batch(vec![VncServerEvent::QemuAudioCapability])
        );
    }

    #[test]
    fn parser_rejects_unknown_subtype_and_operation() {
        assert!(read_qemu_audio_server_message(&mut Cursor::new([2, 0, 0])).is_err());
        assert!(read_qemu_audio_server_message(&mut Cursor::new([1, 0, 3])).is_err());
    }

    #[test]
    fn parser_rejects_oversized_and_unaligned_payloads() {
        let oversized = (MAX_QEMU_AUDIO_PAYLOAD_BYTES as u32 + 4).to_be_bytes();
        let mut oversized_message = vec![1, 0, 2];
        oversized_message.extend_from_slice(&oversized);
        assert!(read_qemu_audio_server_message(&mut Cursor::new(oversized_message)).is_err());
        assert!(
            read_qemu_audio_server_message(&mut Cursor::new([1, 0, 2, 0, 0, 0, 2, 1, 2])).is_err()
        );
    }

    #[test]
    fn parser_reports_truncated_data_without_allocating_more_input() {
        let error = read_qemu_audio_server_message(&mut Cursor::new([1, 0, 2, 0, 0, 0, 4, 1, 2]))
            .unwrap_err();

        assert!(error.contains("payload read failed"));
    }

    #[test]
    fn server_messages_cannot_start_playback_before_confirmation() {
        let mut session = QemuAudioSession::new(true);

        session.handle_server_message(&QemuAudioServerMessage::Start);
        session.handle_server_message(&QemuAudioServerMessage::Data(vec![0; 4]));

        assert!(!session.server_streaming);
    }

    #[test]
    fn user_opt_out_prevents_playback_even_after_server_support() {
        let mut session = QemuAudioSession::new(false);
        session.server_supported = true;
        session.client_enabled = true;

        session.handle_server_message(&QemuAudioServerMessage::Start);

        assert!(!session.server_streaming);
    }
}
