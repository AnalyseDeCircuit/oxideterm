// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::VecDeque;
use std::time::Duration;

use crate::error::ModemTransferError;

pub trait ModemIo {
    fn read_byte(&mut self, timeout: Duration) -> Result<u8, ModemTransferError>;
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ModemTransferError>;
}

#[derive(Debug, Default)]
pub struct MemoryModemIo {
    input: VecDeque<u8>,
    output: Vec<u8>,
}

impl MemoryModemIo {
    pub fn with_input(input: impl Into<Vec<u8>>) -> Self {
        Self {
            input: input.into().into(),
            output: Vec::new(),
        }
    }

    pub fn push_input(&mut self, bytes: &[u8]) {
        self.input.extend(bytes);
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

impl ModemIo for MemoryModemIo {
    fn read_byte(&mut self, _timeout: Duration) -> Result<u8, ModemTransferError> {
        self.input.pop_front().ok_or(ModemTransferError::Timeout)
    }

    fn write_all(&mut self, bytes: &[u8]) -> Result<(), ModemTransferError> {
        self.output.extend_from_slice(bytes);
        Ok(())
    }
}
