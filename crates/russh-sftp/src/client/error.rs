use std::io;
use thiserror::Error;
use tokio::sync::mpsc::error::SendError as MpscSendError;
use tokio::sync::oneshot::error::RecvError as OneshotRecvError;
use tokio::time::error::Elapsed as TimeElapsed;

use crate::error;
use crate::protocol::Status;

/// Enum for client errors
#[derive(Debug, Clone, Error)]
pub enum Error {
    /// Contains an error status packet
    #[error("{}: {}", .0.status_code, .0.error_message)]
    Status(Status),
    /// Any errors related to I/O
    #[error("I/O: {0}")]
    IO(String),
    /// Time limit for receiving response packet exceeded
    #[error("Timeout")]
    Timeout,
    /// Occurs due to exceeding the limits set by the `limits@openssh.com` extension
    #[error("Limit exceeded: {0}")]
    Limited(String),
    /// Occurs when an unexpected packet is sent
    #[error("Unexpected packet")]
    UnexpectedPacket,
    /// Occurs when unexpected server behavior differs from the protocol specifition
    #[error("{0}")]
    UnexpectedBehavior(String),
}

impl From<Status> for Error {
    fn from(status: Status) -> Self {
        Self::Status(status)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::IO(error.to_string())
    }
}

impl<T> From<MpscSendError<T>> for Error {
    fn from(err: MpscSendError<T>) -> Self {
        Self::UnexpectedBehavior(format!("SendError: {}", err))
    }
}

impl From<OneshotRecvError> for Error {
    fn from(err: OneshotRecvError) -> Self {
        Self::UnexpectedBehavior(format!("RecvError: {}", err))
    }
}

impl From<TimeElapsed> for Error {
    fn from(_: TimeElapsed) -> Self {
        Self::Timeout
    }
}

impl From<error::Error> for Error {
    fn from(error: error::Error) -> Self {
        Self::UnexpectedBehavior(error.to_string())
    }
}
