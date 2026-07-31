// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::error::ModemTransferError;
use crate::io::ModemIo;

pub type ModemWakeCallback = Arc<dyn Fn() + Send + Sync + 'static>;

// Keep protocol buffering bounded while allowing several full ZMODEM frames
// to bridge the worker and the terminal transport without blocking the UI.
const MODEM_REMOTE_OUTPUT_BUFFER_BYTES: usize = 4 * 1024 * 1024;
const MODEM_SERVER_WRITE_BUFFER_BYTES: usize = 1024 * 1024;

#[derive(Clone)]
pub struct ModemTransfer {
    inner: Arc<ModemTransferInner>,
}

struct ModemTransferInner {
    state: Mutex<ModemTransferState>,
    available: Condvar,
    wake_host: Option<ModemWakeCallback>,
}

#[derive(Debug, Default)]
struct ModemTransferState {
    remote_output: VecDeque<u8>,
    server_writes: VecDeque<Vec<u8>>,
    server_write_bytes: usize,
    server_write_in_flight_bytes: usize,
    cancellation_bytes: Vec<u8>,
    input_overflow: bool,
    stopped: bool,
}

impl ModemTransfer {
    pub fn new(initial_remote_output: &[u8]) -> Self {
        Self::new_with_wake_and_cancel(initial_remote_output, None, &[])
    }

    pub fn new_with_wake(
        initial_remote_output: &[u8],
        wake_host: Option<ModemWakeCallback>,
    ) -> Self {
        Self::new_with_wake_and_cancel(initial_remote_output, wake_host, &[])
    }

    pub fn new_with_wake_and_cancel(
        initial_remote_output: &[u8],
        wake_host: Option<ModemWakeCallback>,
        cancellation_bytes: &[u8],
    ) -> Self {
        let mut state = ModemTransferState::default();
        state.remote_output.extend(initial_remote_output);
        state
            .cancellation_bytes
            .extend_from_slice(cancellation_bytes);
        Self {
            inner: Arc::new(ModemTransferInner {
                state: Mutex::new(state),
                available: Condvar::new(),
                wake_host,
            }),
        }
    }

    pub fn push_remote_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let mut state = self.inner.state.lock().expect("modem transfer state");
        if state.stopped {
            return;
        }
        if state.remote_output.len().saturating_add(bytes.len()) > MODEM_REMOTE_OUTPUT_BUFFER_BYTES
        {
            state.remote_output.clear();
            state.input_overflow = true;
            Self::cancel_locked(&mut state);
            self.inner.available.notify_all();
            drop(state);
            self.wake_host();
            return;
        }
        state.remote_output.extend(bytes);
        self.inner.available.notify_all();
    }

    pub fn take_server_write(&self) -> Option<Vec<u8>> {
        let mut state = self.inner.state.lock().expect("modem transfer state");
        let bytes = state.server_writes.pop_front()?;
        state.server_write_in_flight_bytes = state
            .server_write_in_flight_bytes
            .saturating_add(bytes.len());
        Some(bytes)
    }

    pub fn complete_server_write(&self, byte_len: usize) {
        let mut state = self.inner.state.lock().expect("modem transfer state");
        state.server_write_in_flight_bytes =
            state.server_write_in_flight_bytes.saturating_sub(byte_len);
        state.server_write_bytes = state.server_write_bytes.saturating_sub(byte_len);
        self.inner.available.notify_all();
    }

    pub fn restore_server_write(&self, bytes: Vec<u8>) {
        let mut state = self.inner.state.lock().expect("modem transfer state");
        state.server_write_in_flight_bytes = state
            .server_write_in_flight_bytes
            .saturating_sub(bytes.len());
        state.server_writes.push_front(bytes);
        self.inner.available.notify_all();
    }

    pub fn server_writes_drained(&self) -> bool {
        let state = self.inner.state.lock().expect("modem transfer state");
        state.server_write_bytes == 0 && state.server_write_in_flight_bytes == 0
    }

    pub fn drain_remote_output(&self) -> Vec<u8> {
        let mut state = self.inner.state.lock().expect("modem transfer state");
        state.remote_output.drain(..).collect()
    }

    pub fn stop(&self) {
        let mut state = self.inner.state.lock().expect("modem transfer state");
        Self::cancel_locked(&mut state);
        self.inner.available.notify_all();
        drop(state);
        self.wake_host();
    }

    fn wake_host(&self) {
        if let Some(wake_host) = &self.inner.wake_host {
            wake_host();
        }
    }

    fn cancel_locked(state: &mut ModemTransferState) {
        if state.stopped {
            return;
        }
        state.stopped = true;
        state.remote_output.clear();

        // Unsent data is no longer useful after cancellation. Replace it with
        // the protocol abort sequence so the peer exits instead of retrying.
        let queued_bytes = state.server_writes.iter().map(Vec::len).sum::<usize>();
        state.server_writes.clear();
        state.server_write_bytes = state.server_write_bytes.saturating_sub(queued_bytes);
        if !state.cancellation_bytes.is_empty() {
            let cancellation = state.cancellation_bytes.clone();
            state.server_write_bytes = state.server_write_bytes.saturating_add(cancellation.len());
            state.server_writes.push_back(cancellation);
        }
    }
}

impl fmt::Debug for ModemTransfer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ModemTransfer")
            .finish_non_exhaustive()
    }
}

impl ModemIo for ModemTransfer {
    fn read_byte(&mut self, timeout: Duration) -> Result<u8, ModemTransferError> {
        let deadline = Instant::now() + timeout;
        let mut state = self.inner.state.lock().expect("modem transfer state");
        loop {
            if state.input_overflow {
                return Err(ModemTransferError::InputBufferOverflow(
                    MODEM_REMOTE_OUTPUT_BUFFER_BYTES,
                ));
            }
            if state.stopped {
                return Err(ModemTransferError::Cancelled);
            }
            if let Some(byte) = state.remote_output.pop_front() {
                return Ok(byte);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(ModemTransferError::Timeout);
            };
            let (new_state, timeout) = self
                .inner
                .available
                .wait_timeout(state, remaining)
                .expect("modem transfer state");
            state = new_state;
            if timeout.timed_out() {
                return Err(ModemTransferError::Timeout);
            }
        }
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ModemTransferError> {
        if bytes.is_empty() {
            return Ok(());
        }
        let mut state = self.inner.state.lock().expect("modem transfer state");
        loop {
            if state.stopped {
                return Err(ModemTransferError::Cancelled);
            }
            let buffered_after_write = state.server_write_bytes.saturating_add(bytes.len());
            let oversized_first_write =
                state.server_write_bytes == 0 && state.server_write_in_flight_bytes == 0;
            if buffered_after_write <= MODEM_SERVER_WRITE_BUFFER_BYTES || oversized_first_write {
                break;
            }
            state = self
                .inner
                .available
                .wait(state)
                .expect("modem transfer state");
        }
        state.server_write_bytes = state.server_write_bytes.saturating_add(bytes.len());
        state.server_writes.push_back(bytes.to_vec());
        drop(state);
        self.wake_host();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    #[test]
    fn transfer_reads_initial_bytes_and_records_writes() {
        let mut transfer = ModemTransfer::new(b"abc");
        assert_eq!(transfer.read_byte(Duration::from_millis(1)).unwrap(), b'a');
        transfer.write_all(b"reply").unwrap();
        let reply = transfer.take_server_write().expect("queued reply");
        assert_eq!(reply, b"reply");
        assert!(!transfer.server_writes_drained());
        transfer.complete_server_write(reply.len());
        assert!(transfer.server_writes_drained());
    }

    #[test]
    fn stop_replaces_unsent_payload_with_protocol_cancellation() {
        let mut transfer = ModemTransfer::new_with_wake_and_cancel(b"", None, b"cancel");
        transfer.write_all(b"payload").unwrap();

        transfer.stop();

        let cancellation = transfer.take_server_write().expect("cancellation bytes");
        assert_eq!(cancellation, b"cancel");
        assert!(matches!(
            transfer.read_byte(Duration::from_millis(1)),
            Err(ModemTransferError::Cancelled)
        ));
    }

    #[test]
    fn restored_server_writes_keep_fifo_order() {
        let mut transfer = ModemTransfer::new(b"");
        transfer.write_all(b"first").unwrap();
        transfer.write_all(b"second").unwrap();

        let first = transfer.take_server_write().expect("first write");
        transfer.restore_server_write(first);

        let first = transfer.take_server_write().expect("restored write");
        assert_eq!(first, b"first");
        transfer.complete_server_write(first.len());
        let second = transfer.take_server_write().expect("second write");
        assert_eq!(second, b"second");
        transfer.complete_server_write(second.len());
        assert!(transfer.server_writes_drained());
    }

    #[test]
    fn outbound_queue_backpressures_until_transport_completes_a_write() {
        let mut transfer = ModemTransfer::new(b"");
        transfer
            .write_all(&vec![0; MODEM_SERVER_WRITE_BUFFER_BYTES])
            .unwrap();
        let (started_tx, started_rx) = mpsc::channel();
        let (completed_tx, completed_rx) = mpsc::channel();
        let mut blocked_transfer = transfer.clone();
        let worker = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            blocked_transfer.write_all(b"next").unwrap();
            completed_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(
            completed_rx
                .recv_timeout(Duration::from_millis(20))
                .is_err()
        );

        let first = transfer.take_server_write().expect("buffer-sized write");
        transfer.complete_server_write(first.len());

        completed_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("producer unblocked");
        worker.join().unwrap();
    }

    #[test]
    fn remote_input_overflow_cancels_with_a_specific_error() {
        let mut transfer = ModemTransfer::new_with_wake_and_cancel(b"", None, b"protocol-cancel");
        transfer.push_remote_output(&vec![0; MODEM_REMOTE_OUTPUT_BUFFER_BYTES + 1]);

        assert!(matches!(
            transfer.read_byte(Duration::from_millis(1)),
            Err(ModemTransferError::InputBufferOverflow(
                MODEM_REMOTE_OUTPUT_BUFFER_BYTES
            ))
        ));
        assert_eq!(
            transfer.take_server_write().as_deref(),
            Some(b"protocol-cancel".as_slice())
        );
    }
}
