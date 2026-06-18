// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::error::ModemTransferError;
use crate::io::ModemIo;

pub type ModemWakeCallback = Arc<dyn Fn() + Send + Sync + 'static>;

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
    server_writes: Vec<Vec<u8>>,
    stopped: bool,
}

impl ModemTransfer {
    pub fn new(initial_remote_output: &[u8]) -> Self {
        Self::new_with_wake(initial_remote_output, None)
    }

    pub fn new_with_wake(
        initial_remote_output: &[u8],
        wake_host: Option<ModemWakeCallback>,
    ) -> Self {
        let mut state = ModemTransferState::default();
        state.remote_output.extend(initial_remote_output);
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
        state.remote_output.extend(bytes);
        self.inner.available.notify_all();
    }

    pub fn take_server_writes(&self) -> Vec<Vec<u8>> {
        let mut state = self.inner.state.lock().expect("modem transfer state");
        std::mem::take(&mut state.server_writes)
    }

    pub fn stop(&self) {
        let mut state = self.inner.state.lock().expect("modem transfer state");
        state.stopped = true;
        self.inner.available.notify_all();
        drop(state);
        self.wake_host();
    }

    fn wake_host(&self) {
        if let Some(wake_host) = &self.inner.wake_host {
            wake_host();
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
            if let Some(byte) = state.remote_output.pop_front() {
                return Ok(byte);
            }
            if state.stopped {
                return Err(ModemTransferError::Cancelled);
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
        if state.stopped {
            return Err(ModemTransferError::Cancelled);
        }
        state.server_writes.push(bytes.to_vec());
        drop(state);
        self.wake_host();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_reads_initial_bytes_and_records_writes() {
        let mut transfer = ModemTransfer::new(b"abc");
        assert_eq!(transfer.read_byte(Duration::from_millis(1)).unwrap(), b'a');
        transfer.write_all(b"reply").unwrap();
        assert_eq!(transfer.take_server_writes(), vec![b"reply".to_vec()]);
    }
}
