// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

mod listing;
mod session;
mod transfer;

pub use listing::{Entry, EntryKind};
pub use session::{ConnectOptions, FtpSession, Security};
pub use transfer::TransferProgress;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("FTP operation cancelled")]
    Cancelled,
    #[error("FTP operation timed out")]
    Timeout,
    #[error("FTP connection requires reconnection")]
    Disconnected,
    #[error("Invalid FTP connection or path")]
    InvalidInput,
    #[error("FTP server rejected the operation ({0})")]
    Server(u32),
    #[error("FTP transport or protocol failure")]
    Protocol,
    #[error("FTPS certificate or TLS handshake failed")]
    Tls,
    #[error("FTP local file operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("FTP directory listing contains unsupported entries")]
    Listing,
    #[error("Remote file exceeds the preview limit")]
    TooLarge,
}

impl From<suppaftp::FtpError> for Error {
    fn from(error: suppaftp::FtpError) -> Self {
        // Server replies can echo commands and credentials. Keep only a status code.
        match error {
            suppaftp::FtpError::UnexpectedResponse(response) => {
                Self::Server(response.status as u32)
            }
            suppaftp::FtpError::SecureError(_) => Self::Tls,
            _ => Self::Protocol,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests;
