// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Terminal-native X/Y/ZMODEM protocol primitives.
//!
//! This crate owns byte-level modem protocol state that can be fed from
//! terminal PTY chunks. It intentionally avoids blocking file descriptors,
//! GPUI types, and direct filesystem side effects.

pub mod consumer;
pub mod crc;
pub mod detector;
pub mod error;
pub mod io;
pub mod stream;
pub mod xymodem;
pub mod xymodem_transfer;
pub mod zmodem;
pub mod zmodem_transfer;

pub use consumer::{
    ModemConsumer, ModemConsumerEvent, ModemTransferDirection, ModemTransferRequest,
};
pub use detector::{DetectedModemProtocol, ModemDetector};
pub use error::{ModemError, ModemTransferError};
pub use io::{MemoryModemIo, ModemIo};
pub use stream::{ModemTransfer, ModemWakeCallback};
