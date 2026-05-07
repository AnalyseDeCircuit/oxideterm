// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SftpError {
    #[error("SFTP subsystem not available: {0}")]
    SubsystemNotAvailable(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("File not found: {0}")]
    FileNotFound(String),
    #[error("Directory not found: {0}")]
    DirectoryNotFound(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Channel error: {0}")]
    ChannelError(String),
    #[error("Protocol error: {0}")]
    ProtocolError(String),
    #[error("Invalid path: {0}")]
    InvalidPath(String),
    #[error("Transfer cancelled")]
    TransferCancelled,
    #[error("SFTP session not initialized for: {0}")]
    NotInitialized(String),
    #[error("Transfer error: {0}")]
    TransferError(String),
    #[error("Write error: {0}")]
    WriteError(String),
    #[error("Storage error: {0}")]
    StorageError(String),
}

impl SftpError {
    pub fn is_channel_recoverable(&self) -> bool {
        match self {
            Self::ChannelError(_) | Self::ProtocolError(_) | Self::SubsystemNotAvailable(_) => true,
            Self::IoError(error) => matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionReset
                    | std::io::ErrorKind::ConnectionAborted
                    | std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::TimedOut
                    | std::io::ErrorKind::UnexpectedEof
            ),
            Self::PermissionDenied(_)
            | Self::FileNotFound(_)
            | Self::DirectoryNotFound(_)
            | Self::InvalidPath(_)
            | Self::TransferCancelled
            | Self::NotInitialized(_)
            | Self::TransferError(_)
            | Self::WriteError(_)
            | Self::StorageError(_) => false,
        }
    }
}

impl serde::Serialize for SftpError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
