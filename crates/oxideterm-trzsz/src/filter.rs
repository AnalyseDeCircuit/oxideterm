// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;
use std::sync::OnceLock;

use regex::Regex;

use crate::types::{
    TrzszDetectedHandshake, TrzszHandshakeMode, TrzszTransferDirection, TrzszTransferPolicy,
};

pub const TRZSZ_MAGIC_KEY_PREFIX: &str = "::TRZSZ:TRANSFER:";
pub const MAGIC_DETECT_BUFFER_BYTES: usize = 128;
pub const DRAG_INIT_TIMEOUT_MESSAGE: &str = "Upload does not start";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrzszFilterOutput {
    WriteTerminal(Vec<u8>),
    SendServer(Vec<u8>),
    TransferData(Vec<u8>),
    StartTransfer(TrzszDetectedHandshake),
    CancelTransfer,
    UploadTimedOut { message: &'static str },
}

#[derive(Debug, Clone)]
pub struct TrzszFilter {
    detect_tail: Vec<u8>,
    pending_detect_buffers: Vec<String>,
    unique_ids: HashMap<String, usize>,
    transfer_active: bool,
    disposed: bool,
    upload_interrupting: bool,
    upload_skip_trz_command: bool,
    upload_init_pending: bool,
    upload_init_has_directory: bool,
    terminal_columns: usize,
    transfer_policy: TrzszTransferPolicy,
}

impl Default for TrzszFilter {
    fn default() -> Self {
        Self::new(TrzszTransferPolicy::default())
    }
}

impl TrzszFilter {
    pub fn new(transfer_policy: TrzszTransferPolicy) -> Self {
        Self {
            detect_tail: Vec::new(),
            pending_detect_buffers: Vec::new(),
            unique_ids: HashMap::new(),
            transfer_active: false,
            disposed: false,
            upload_interrupting: false,
            upload_skip_trz_command: false,
            upload_init_pending: false,
            upload_init_has_directory: false,
            terminal_columns: 80,
            transfer_policy,
        }
    }

    pub fn transfer_policy(&self) -> &TrzszTransferPolicy {
        &self.transfer_policy
    }

    pub fn update_transfer_policy(&mut self, transfer_policy: TrzszTransferPolicy) {
        self.transfer_policy = transfer_policy;
    }

    pub fn set_terminal_columns(&mut self, columns: usize) {
        if columns > 0 {
            self.terminal_columns = columns;
        }
    }

    pub fn terminal_columns(&self) -> usize {
        self.terminal_columns
    }

    pub fn is_transferring_files(&self) -> bool {
        self.transfer_active
    }

    pub fn begin_transfer_for_detected_handshake(&mut self, handshake: TrzszDetectedHandshake) {
        if handshake.direction == TrzszTransferDirection::Upload {
            self.upload_init_pending = false;
        }
        self.transfer_active = true;
    }

    pub fn finish_transfer(&mut self) {
        self.transfer_active = false;
    }

    pub fn dispose(&mut self) -> Vec<TrzszFilterOutput> {
        if self.disposed {
            return Vec::new();
        }
        self.disposed = true;
        let mut output = Vec::new();
        if self.transfer_active {
            output.push(TrzszFilterOutput::CancelTransfer);
        }
        self.transfer_active = false;
        self.upload_init_pending = false;
        output
    }

    pub fn process_server_output(&mut self, output: &[u8]) -> Vec<TrzszFilterOutput> {
        if self.disposed {
            return Vec::new();
        }

        if self.transfer_active {
            return vec![TrzszFilterOutput::TransferData(output.to_vec())];
        }

        if self.upload_interrupting {
            return Vec::new();
        }

        let terminal_output = if self.upload_skip_trz_command {
            self.upload_skip_trz_command = false;
            let stripped = strip_server_output(output);
            if matches!(stripped.as_deref(), Some("trz" | "trz -d")) {
                return vec![TrzszFilterOutput::WriteTerminal(b"\r\n".to_vec())];
            }
            stripped.map_or_else(|| output.to_vec(), |value| value.into_bytes())
        } else {
            output.to_vec()
        };

        self.queue_magic_detection(&terminal_output);
        vec![TrzszFilterOutput::WriteTerminal(terminal_output)]
    }

    pub fn drain_detected_handshakes(&mut self, is_windows_shell: bool) -> Vec<TrzszFilterOutput> {
        if self.disposed || self.transfer_active {
            self.pending_detect_buffers.clear();
            return Vec::new();
        }

        let buffers = std::mem::take(&mut self.pending_detect_buffers);
        let mut output = Vec::new();
        for buffer in buffers {
            let Some(handshake) = parse_trzsz_handshake(&buffer, is_windows_shell) else {
                continue;
            };
            if self.unique_id_exists(&handshake.unique_id, is_windows_shell) {
                continue;
            }

            // The Tauri filter locks the handshake by installing TrzszTransfer
            // before any async file chooser work. Native keeps the same edge:
            // no later magic key may start another transfer until this one ends.
            self.begin_transfer_for_detected_handshake(handshake.clone());
            output.push(TrzszFilterOutput::StartTransfer(handshake));
            break;
        }
        output
    }

    pub fn process_terminal_input(&mut self, input: &str) -> Option<TrzszFilterOutput> {
        if self.disposed {
            return None;
        }

        if self.transfer_active {
            if input == "\x03" {
                return Some(TrzszFilterOutput::CancelTransfer);
            }
            return None;
        }

        Some(TrzszFilterOutput::SendServer(input.as_bytes().to_vec()))
    }

    pub fn process_binary_input(&mut self, input: &str) -> Option<TrzszFilterOutput> {
        if self.disposed || self.transfer_active {
            return None;
        }

        let bytes = input.chars().map(|value| value as u32 as u8).collect();
        Some(TrzszFilterOutput::SendServer(bytes))
    }

    pub fn begin_upload_interrupt(&mut self, has_directory: bool) -> Option<TrzszFilterOutput> {
        if self.disposed || self.transfer_active || self.upload_init_pending {
            return None;
        }

        self.upload_interrupting = true;
        self.upload_init_pending = true;
        self.upload_init_has_directory = has_directory;
        Some(TrzszFilterOutput::SendServer(vec![0x03]))
    }

    pub fn finish_upload_interrupt(&mut self) -> Option<TrzszFilterOutput> {
        if self.disposed || !self.upload_init_pending {
            return None;
        }

        self.upload_interrupting = false;
        self.upload_skip_trz_command = true;
        let command = if self.upload_init_has_directory {
            b"trz -d\r".to_vec()
        } else {
            b"trz\r".to_vec()
        };
        Some(TrzszFilterOutput::SendServer(command))
    }

    pub fn upload_init_timed_out(&mut self) -> Option<TrzszFilterOutput> {
        if !self.upload_init_pending {
            return None;
        }
        self.upload_init_pending = false;
        self.upload_interrupting = false;
        self.upload_skip_trz_command = false;
        Some(TrzszFilterOutput::UploadTimedOut {
            message: DRAG_INIT_TIMEOUT_MESSAGE,
        })
    }

    fn queue_magic_detection(&mut self, output: &[u8]) {
        let mut buffer = Vec::with_capacity(self.detect_tail.len() + output.len());
        buffer.extend_from_slice(&self.detect_tail);
        buffer.extend_from_slice(output);

        if let Some(magic) = find_trzsz_magic_key_bytes(&buffer) {
            // Tauri waits 10ms before handling a detected magic key so the
            // terminal can flush surrounding bytes. Scheduling that timer is a
            // UI/runtime concern; the crate preserves it as an explicit drain.
            self.pending_detect_buffers.push(magic);
        }

        if buffer.len() > MAGIC_DETECT_BUFFER_BYTES {
            self.detect_tail = buffer[buffer.len() - MAGIC_DETECT_BUFFER_BYTES..].to_vec();
        } else {
            self.detect_tail = buffer;
        }
    }

    fn unique_id_exists(&mut self, unique_id: &str, is_windows_shell: bool) -> bool {
        if unique_id.is_empty() {
            return false;
        }

        if !is_windows_shell && unique_id.len() == 14 && unique_id.ends_with("00") {
            return false;
        }

        if self.unique_ids.contains_key(unique_id) {
            return true;
        }

        if self.unique_ids.len() >= 100 {
            self.unique_ids.retain(|_, index| *index >= 50);
            for index in self.unique_ids.values_mut() {
                *index -= 50;
            }
        }

        self.unique_ids
            .insert(unique_id.to_string(), self.unique_ids.len());
        false
    }
}

pub fn find_trzsz_magic_key_bytes(output: &[u8]) -> Option<String> {
    let prefix = TRZSZ_MAGIC_KEY_PREFIX.as_bytes();
    if output.len() < prefix.len() {
        return None;
    }

    let index = output
        .windows(prefix.len())
        .rposition(|window| window == prefix)?;
    Some(String::from_utf8_lossy(&output[index..]).into_owned())
}

pub fn find_trzsz_magic_key_str(output: &str) -> Option<String> {
    let index = output.rfind(TRZSZ_MAGIC_KEY_PREFIX)?;
    Some(output[index..].to_string())
}

pub fn parse_trzsz_handshake(
    output: &str,
    is_windows_shell: bool,
) -> Option<TrzszDetectedHandshake> {
    static MAGIC_RE: OnceLock<Regex> = OnceLock::new();
    let regex = MAGIC_RE.get_or_init(|| {
        Regex::new(r"::TRZSZ:TRANSFER:([SRD]):(\d+\.\d+\.\d+)(:\d+)?(?::\d+)?")
            .expect("valid trzsz magic regex")
    });

    let captures = regex.captures(output)?;
    let whole_match = captures.get(0)?;
    if output
        .as_bytes()
        .get(whole_match.end())
        .is_some_and(u8::is_ascii_digit)
    {
        return None;
    }
    let mode = captures
        .get(1)
        .and_then(|value| value.as_str().chars().next())
        .and_then(TrzszHandshakeMode::from_protocol_char)?;
    let version = captures.get(2)?.as_str();
    let unique_id = captures.get(3).map_or("", |value| value.as_str());

    Some(TrzszDetectedHandshake::from_parts(
        mode,
        version,
        unique_id,
        is_windows_shell,
    ))
}

fn strip_server_output(output: &[u8]) -> Option<String> {
    let mut buffer = Vec::with_capacity(output.len());
    let mut skip_vt100 = false;

    for char_code in output {
        if skip_vt100 {
            if is_vt100_end(*char_code) {
                skip_vt100 = false;
            }
            continue;
        }

        if *char_code == 0x1b {
            skip_vt100 = true;
            continue;
        }

        buffer.push(*char_code);
    }

    while matches!(buffer.last(), Some(0x0d | 0x0a)) {
        buffer.pop();
    }

    if buffer.len() > 100 {
        return None;
    }

    Some(String::from_utf8_lossy(&buffer).into_owned())
}

fn is_vt100_end(char_code: u8) -> bool {
    char_code.is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{TrzszTransferDirection, TrzszTransferSelection};

    #[test]
    fn detects_magic_key_split_across_chunks() {
        let mut filter = TrzszFilter::default();
        assert_eq!(
            filter.process_server_output(b"hello ::TRZSZ:TRANS"),
            vec![TrzszFilterOutput::WriteTerminal(
                b"hello ::TRZSZ:TRANS".to_vec()
            )]
        );
        let _ = filter.process_server_output(b"FER:S:1.1.6:123\r\n");

        let output = filter.drain_detected_handshakes(false);
        assert_eq!(output.len(), 1);
        let TrzszFilterOutput::StartTransfer(handshake) = &output[0] else {
            panic!("expected start transfer");
        };
        assert_eq!(handshake.mode, TrzszHandshakeMode::Send);
        assert_eq!(handshake.direction, TrzszTransferDirection::Download);
        assert_eq!(handshake.selection, TrzszTransferSelection::File);
        assert_eq!(handshake.unique_id, ":123");
    }

    #[test]
    fn duplicate_unique_ids_are_ignored() {
        let mut filter = TrzszFilter::default();
        let _ = filter.process_server_output(b"::TRZSZ:TRANSFER:R:1.1.6:9\r\n");
        assert_eq!(filter.drain_detected_handshakes(false).len(), 1);
        filter.finish_transfer();

        let _ = filter.process_server_output(b"::TRZSZ:TRANSFER:R:1.1.6:9\r\n");
        assert!(filter.drain_detected_handshakes(false).is_empty());
    }

    #[test]
    fn non_windows_trailing_zero_unique_ids_are_not_deduplicated() {
        let mut filter = TrzszFilter::default();
        let magic = b"::TRZSZ:TRANSFER:R:1.1.6:1234567890100\r\n";
        let _ = filter.process_server_output(magic);
        assert_eq!(filter.drain_detected_handshakes(false).len(), 1);
        filter.finish_transfer();

        let _ = filter.process_server_output(magic);
        assert_eq!(filter.drain_detected_handshakes(false).len(), 1);
    }

    #[test]
    fn transfer_swallow_inputs_except_ctrl_c() {
        let mut filter = TrzszFilter::default();
        let handshake = parse_trzsz_handshake("::TRZSZ:TRANSFER:R:1.1.6:1", false).unwrap();
        filter.begin_transfer_for_detected_handshake(handshake);

        assert_eq!(filter.process_terminal_input("abc"), None);
        assert_eq!(filter.process_binary_input("abc"), None);
        assert_eq!(
            filter.process_terminal_input("\x03"),
            Some(TrzszFilterOutput::CancelTransfer)
        );
    }

    #[test]
    fn idle_inputs_are_forwarded_to_server() {
        let mut filter = TrzszFilter::default();
        assert_eq!(
            filter.process_terminal_input("ls\r"),
            Some(TrzszFilterOutput::SendServer(b"ls\r".to_vec()))
        );
        assert_eq!(
            filter.process_binary_input("\u{0101}A"),
            Some(TrzszFilterOutput::SendServer(vec![1, 65]))
        );
    }

    #[test]
    fn upload_interrupt_sequence_matches_tauri() {
        let mut filter = TrzszFilter::default();
        assert_eq!(
            filter.begin_upload_interrupt(true),
            Some(TrzszFilterOutput::SendServer(vec![0x03]))
        );
        assert!(filter.process_server_output(b"noise").is_empty());
        assert_eq!(
            filter.finish_upload_interrupt(),
            Some(TrzszFilterOutput::SendServer(b"trz -d\r".to_vec()))
        );
        assert_eq!(
            filter.process_server_output(b"\x1b[31mtrz -d\r\n"),
            vec![TrzszFilterOutput::WriteTerminal(b"\r\n".to_vec())]
        );
    }

    #[test]
    fn upload_init_timeout_uses_tauri_message() {
        let mut filter = TrzszFilter::default();
        let _ = filter.begin_upload_interrupt(false);
        assert_eq!(
            filter.upload_init_timed_out(),
            Some(TrzszFilterOutput::UploadTimedOut {
                message: DRAG_INIT_TIMEOUT_MESSAGE
            })
        );
        assert_eq!(filter.upload_init_timed_out(), None);
    }
}
