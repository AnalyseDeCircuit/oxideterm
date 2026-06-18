// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ModemError {
    #[error("the frame is incomplete")]
    Incomplete,
    #[error("the frame marker is invalid")]
    InvalidMarker,
    #[error("the frame type is invalid")]
    InvalidFrameType,
    #[error("the frame encoding is invalid")]
    InvalidFrameEncoding,
    #[error("the block number complement is invalid")]
    InvalidBlockNumber,
    #[error("the checksum is invalid")]
    InvalidChecksum,
    #[error("the CRC is invalid")]
    InvalidCrc,
    #[error("the payload length is invalid")]
    InvalidLength,
    #[error("the escaped byte sequence is invalid")]
    InvalidEscape,
    #[error("the YMODEM header contains an invalid filename")]
    InvalidFileName,
}

#[derive(Debug, Error)]
pub enum ModemTransferError {
    #[error(transparent)]
    Protocol(#[from] ModemError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("the modem peer timed out")]
    Timeout,
    #[error("the modem peer cancelled the transfer")]
    Cancelled,
    #[error("the modem peer sent an unexpected byte: 0x{0:02x}")]
    UnexpectedByte(u8),
    #[error("the modem peer sent an unexpected frame")]
    UnexpectedFrame,
    #[error("the file is too large for this modem protocol: {0} bytes")]
    UnsupportedFileSize(u64),
}
