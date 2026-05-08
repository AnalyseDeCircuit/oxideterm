// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::sync::{Arc, Mutex};

use crate::filter::{TrzszFilter, TrzszFilterOutput};
use crate::transfer::{DEFAULT_MAX_DATA_CHUNK_SIZE, TrzszTransfer, TrzszTransferInput};
use crate::types::{TrzszDetectedHandshake, TrzszTransferPolicy};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrzszConsumerEvent {
    WriteTerminal(Vec<u8>),
    SendServer(Vec<u8>),
    TransferStarted(TrzszDetectedHandshake),
    TransferDataQueued,
    TransferCancelRequested,
    UploadTimedOut { message: &'static str },
}

pub struct TrzszConsumer {
    filter: TrzszFilter,
    transfer: Option<TrzszTransfer>,
    transfer_input: Option<TrzszTransferInput>,
    server_writes: Arc<Mutex<Vec<Vec<u8>>>>,
    is_windows_shell: bool,
    max_data_chunk_size: usize,
}

impl TrzszConsumer {
    pub fn new(transfer_policy: TrzszTransferPolicy) -> Self {
        let max_data_chunk_size = transfer_policy
            .max_chunk_bytes
            .clamp(1024, DEFAULT_MAX_DATA_CHUNK_SIZE);
        Self::new_with_platform(transfer_policy, false, max_data_chunk_size)
    }

    pub fn new_with_platform(
        transfer_policy: TrzszTransferPolicy,
        is_windows_shell: bool,
        max_data_chunk_size: usize,
    ) -> Self {
        Self {
            filter: TrzszFilter::new(transfer_policy),
            transfer: None,
            transfer_input: None,
            server_writes: Arc::new(Mutex::new(Vec::new())),
            is_windows_shell,
            max_data_chunk_size,
        }
    }

    pub fn filter(&self) -> &TrzszFilter {
        &self.filter
    }

    pub fn filter_mut(&mut self) -> &mut TrzszFilter {
        &mut self.filter
    }

    pub fn active_transfer(&self) -> Option<&TrzszTransfer> {
        self.transfer.as_ref()
    }

    pub fn active_transfer_mut(&mut self) -> Option<&mut TrzszTransfer> {
        self.transfer.as_mut()
    }

    pub fn is_transferring(&self) -> bool {
        (self.transfer.is_some() || self.transfer_input.is_some())
            && self.filter.is_transferring_files()
    }

    pub fn finish_transfer(&mut self) {
        self.transfer = None;
        self.transfer_input = None;
        self.filter.finish_transfer();
    }

    pub fn interrupt_transfer(&mut self) {
        if let Some(transfer) = self.transfer.as_mut() {
            transfer.stop_transferring();
        }
        if let Some(input) = self.transfer_input.as_ref() {
            input.stop_transferring();
        }
        self.finish_transfer();
    }

    pub fn take_active_transfer(&mut self) -> Option<TrzszTransfer> {
        self.transfer.take()
    }

    pub fn update_transfer_policy(&mut self, transfer_policy: TrzszTransferPolicy) {
        self.max_data_chunk_size = transfer_policy
            .max_chunk_bytes
            .clamp(1024, DEFAULT_MAX_DATA_CHUNK_SIZE);
        self.filter.update_transfer_policy(transfer_policy);
    }

    pub fn take_server_writes(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut *self.server_writes.lock().expect("trzsz server writes"))
    }

    pub fn process_server_output(&mut self, output: &[u8]) -> Vec<TrzszConsumerEvent> {
        let filter_output = self.filter.process_server_output(output);
        self.consume_filter_outputs(filter_output)
    }

    pub fn drain_detected_handshakes(&mut self) -> Vec<TrzszConsumerEvent> {
        let filter_output = self.filter.drain_detected_handshakes(self.is_windows_shell);
        self.consume_filter_outputs(filter_output)
    }

    pub fn process_terminal_input(&mut self, input: &str) -> Vec<TrzszConsumerEvent> {
        let Some(output) = self.filter.process_terminal_input(input) else {
            return Vec::new();
        };
        self.consume_filter_outputs(vec![output])
    }

    pub fn process_binary_input(&mut self, input: &str) -> Vec<TrzszConsumerEvent> {
        let Some(output) = self.filter.process_binary_input(input) else {
            return Vec::new();
        };
        self.consume_filter_outputs(vec![output])
    }

    pub fn begin_upload_interrupt(&mut self, has_directory: bool) -> Vec<TrzszConsumerEvent> {
        let Some(output) = self.filter.begin_upload_interrupt(has_directory) else {
            return Vec::new();
        };
        self.consume_filter_outputs(vec![output])
    }

    pub fn finish_upload_interrupt(&mut self) -> Vec<TrzszConsumerEvent> {
        let Some(output) = self.filter.finish_upload_interrupt() else {
            return Vec::new();
        };
        self.consume_filter_outputs(vec![output])
    }

    pub fn upload_init_timed_out(&mut self) -> Vec<TrzszConsumerEvent> {
        let Some(output) = self.filter.upload_init_timed_out() else {
            return Vec::new();
        };
        self.consume_filter_outputs(vec![output])
    }

    fn consume_filter_outputs(
        &mut self,
        outputs: Vec<TrzszFilterOutput>,
    ) -> Vec<TrzszConsumerEvent> {
        let mut events = Vec::new();
        for output in outputs {
            match output {
                TrzszFilterOutput::WriteTerminal(bytes) => {
                    events.push(TrzszConsumerEvent::WriteTerminal(bytes));
                }
                TrzszFilterOutput::SendServer(bytes) => {
                    events.push(TrzszConsumerEvent::SendServer(bytes));
                }
                TrzszFilterOutput::TransferData(bytes) => {
                    if let Some(transfer) = self.transfer.as_mut() {
                        transfer.add_received_data(&bytes);
                    } else if let Some(input) = self.transfer_input.as_ref() {
                        input.add_received_data(&bytes);
                    }
                    events.push(TrzszConsumerEvent::TransferDataQueued);
                }
                TrzszFilterOutput::StartTransfer(handshake) => {
                    self.start_transfer(handshake.clone());
                    events.push(TrzszConsumerEvent::TransferStarted(handshake));
                }
                TrzszFilterOutput::CancelTransfer => {
                    if let Some(transfer) = self.transfer.as_mut() {
                        transfer.stop_transferring();
                    }
                    if let Some(input) = self.transfer_input.as_ref() {
                        input.stop_transferring();
                    }
                    events.push(TrzszConsumerEvent::TransferCancelRequested);
                }
                TrzszFilterOutput::UploadTimedOut { message } => {
                    events.push(TrzszConsumerEvent::UploadTimedOut { message });
                }
            }
        }
        events
    }

    fn start_transfer(&mut self, handshake: TrzszDetectedHandshake) {
        let output = self.server_writes.clone();
        let mut transfer = TrzszTransfer::new(
            move |bytes| output.lock().expect("trzsz server writes").push(bytes),
            self.is_windows_shell,
            self.max_data_chunk_size,
        );
        transfer.set_remote_platform(handshake.remote_is_windows);
        self.transfer_input = Some(transfer.input_handle());
        self.transfer = Some(transfer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::DRAG_INIT_TIMEOUT_MESSAGE;

    #[test]
    fn consumer_starts_transfer_and_routes_following_server_data_to_transfer() {
        let mut consumer = TrzszConsumer::new(TrzszTransferPolicy::default());
        let events = consumer.process_server_output(b"::TRZSZ:TRANSFER:R:1.1.6:7\n");
        assert!(matches!(
            events.as_slice(),
            [TrzszConsumerEvent::WriteTerminal(_)]
        ));
        let events = consumer.drain_detected_handshakes();
        assert!(matches!(
            events.as_slice(),
            [TrzszConsumerEvent::TransferStarted(_)]
        ));
        assert!(consumer.active_transfer().is_some());

        let config = crate::transfer::tests_support::encode_buffer_for_tests(
            br#"{"binary":false,"directory":false}"#,
        );
        let events = consumer.process_server_output(format!("#CFG:{config}\n").as_bytes());
        assert_eq!(events, vec![TrzszConsumerEvent::TransferDataQueued]);
        let config = consumer
            .active_transfer_mut()
            .expect("active transfer")
            .recv_config()
            .unwrap();
        assert_eq!(config["binary"], serde_json::Value::Bool(false));
    }

    #[test]
    fn consumer_ctrl_c_stops_active_transfer() {
        let mut consumer = TrzszConsumer::new(TrzszTransferPolicy::default());
        let _ = consumer.process_server_output(b"::TRZSZ:TRANSFER:R:1.1.6:8\n");
        let _ = consumer.drain_detected_handshakes();
        let events = consumer.process_terminal_input("\x03");
        assert_eq!(events, vec![TrzszConsumerEvent::TransferCancelRequested]);
        assert!(
            consumer
                .active_transfer_mut()
                .expect("active transfer")
                .recv_config()
                .is_err()
        );
    }

    #[test]
    fn taken_transfer_still_receives_server_data_through_input_handle() {
        let mut consumer = TrzszConsumer::new(TrzszTransferPolicy::default());
        let _ = consumer.process_server_output(b"::TRZSZ:TRANSFER:R:1.1.6:9\n");
        let _ = consumer.drain_detected_handshakes();
        let mut transfer = consumer
            .take_active_transfer()
            .expect("worker should take transfer owner");

        let config = crate::transfer::tests_support::encode_buffer_for_tests(
            br#"{"binary":false,"directory":false}"#,
        );
        let events = consumer.process_server_output(format!("#CFG:{config}\n").as_bytes());
        assert_eq!(events, vec![TrzszConsumerEvent::TransferDataQueued]);

        let config = transfer.recv_config().expect("config through input handle");
        assert_eq!(config["binary"], serde_json::Value::Bool(false));
        consumer.finish_transfer();
        assert!(!consumer.is_transferring());
    }

    #[test]
    fn consumer_maps_upload_interrupt_outputs() {
        let mut consumer = TrzszConsumer::new(TrzszTransferPolicy::default());
        assert_eq!(
            consumer.begin_upload_interrupt(false),
            vec![TrzszConsumerEvent::SendServer(vec![0x03])]
        );
        assert_eq!(
            consumer.finish_upload_interrupt(),
            vec![TrzszConsumerEvent::SendServer(b"trz\r".to_vec())]
        );
        let mut consumer = TrzszConsumer::new(TrzszTransferPolicy::default());
        let _ = consumer.begin_upload_interrupt(false);
        assert_eq!(
            consumer.upload_init_timed_out(),
            vec![TrzszConsumerEvent::UploadTimedOut {
                message: DRAG_INIT_TIMEOUT_MESSAGE
            }]
        );
    }
}
