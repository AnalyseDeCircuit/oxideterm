// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

pub type X11Result<T> = Result<T, X11ForwardingError>;

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum X11ForwardingError {
    #[error("DISPLAY is empty")]
    EmptyDisplay,
    #[error("DISPLAY is not set")]
    MissingDisplay,
    #[error("invalid DISPLAY value: {0}")]
    InvalidDisplay(String),
    #[error("X11 TCP display port is out of range for display {0}")]
    DisplayPortOutOfRange(u16),
    #[error("invalid X11 auth cookie: {0}")]
    InvalidAuthCookie(String),
    #[error("unsupported X11 auth protocol: {0}")]
    UnsupportedAuthProtocol(String),
    #[error("invalid xauth record: {0}")]
    InvalidXauthRecord(&'static str),
    #[error("no MIT-MAGIC-COOKIE-1 xauth entry matched DISPLAY")]
    MissingAuthEntry,
    #[error("X11 authority file is unavailable: {0}")]
    AuthorityFileUnavailable(String),
    #[error("no remote X11 display candidate was available")]
    RemoteDisplayUnavailable,
    #[error("X11 auth cookie did not match the forwarding cookie")]
    AuthCookieMismatch,
    #[error("X11 setup packet is incomplete")]
    IncompleteSetupPacket,
    #[error("X11 setup packet exceeded {0} bytes before authentication completed")]
    SetupPacketTooLarge(usize),
    #[error("unsupported X11 byte order marker: {0}")]
    UnsupportedByteOrder(u8),
    #[error("X11 setup packet length is invalid")]
    InvalidSetupPacketLength,
}
