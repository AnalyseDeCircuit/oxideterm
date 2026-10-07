// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Versioned private pipe between the terminal owner and the Mosh engine.

use crate::MoshIpFamily;
use serde::{Deserialize, Serialize};
use std::io;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use zeroize::Zeroizing;

pub const PROTOCOL_VERSION: u32 = 1;
pub const MAX_FRAME_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Interrupted,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShutdownOutcome {
    Acknowledged,
    PeerRequested,
    TimedOut,
}

#[derive(Serialize, Deserialize)]
pub enum Request {
    Start {
        version: u32,
        host: String,
        port: u16,
        family: MoshIpFamily,
        columns: u16,
        rows: u16,
        key: Zeroizing<String>,
    },
    Input {
        prediction_id: u64,
        bytes: Zeroizing<Vec<u8>>,
    },
    Resize {
        columns: u16,
        rows: u16,
    },
    Shutdown,
}

#[derive(Serialize, Deserialize)]
pub enum Event {
    Ready { version: u32 },
    Output(Zeroizing<Vec<u8>>),
    RemoteResize { columns: u16, rows: u16 },
    ConnectionStateChanged(ConnectionState),
    RoundTripEstimate(u16),
    PredictionAcknowledged(u64),
    RemoteStateAdvanced(u64),
    Closed(ShutdownOutcome),
    Failed,
}

// Input and output stay raw bytes; JSON is used only for small control frames.
pub trait Frame: Sized + Serialize + for<'de> Deserialize<'de> {
    fn raw(&self) -> Option<(u8, Zeroizing<Vec<u8>>)>;
    fn from_raw(kind: u8, bytes: &[u8]) -> io::Result<Self>;
}

impl Frame for Request {
    fn raw(&self) -> Option<(u8, Zeroizing<Vec<u8>>)> {
        if let Self::Input {
            prediction_id,
            bytes,
        } = self
        {
            let mut payload = Zeroizing::new(prediction_id.to_be_bytes().to_vec());
            payload.extend_from_slice(bytes);
            Some((1, payload))
        } else {
            None
        }
    }
    fn from_raw(kind: u8, bytes: &[u8]) -> io::Result<Self> {
        if kind != 1 || bytes.len() < 8 {
            return Err(invalid());
        }
        Ok(Self::Input {
            prediction_id: u64::from_be_bytes(bytes[..8].try_into().unwrap()),
            bytes: Zeroizing::new(bytes[8..].to_vec()),
        })
    }
}

impl Frame for Event {
    fn raw(&self) -> Option<(u8, Zeroizing<Vec<u8>>)> {
        if let Self::Output(bytes) = self {
            Some((2, bytes.clone()))
        } else {
            None
        }
    }
    fn from_raw(kind: u8, bytes: &[u8]) -> io::Result<Self> {
        if kind != 2 {
            return Err(invalid());
        }
        Ok(Self::Output(Zeroizing::new(bytes.to_vec())))
    }
}

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "Invalid Mosh plugin frame")
}

pub async fn write_frame<W: AsyncWrite + Unpin, T: Frame>(
    writer: &mut W,
    value: &T,
) -> io::Result<()> {
    let (kind, bytes) = match value.raw() {
        Some(raw) => raw,
        None => (
            0,
            Zeroizing::new(serde_json::to_vec(value).map_err(|_| invalid())?),
        ),
    };
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(invalid());
    }
    writer.write_u8(kind).await?;
    writer.write_u32(bytes.len() as u32).await?;
    writer.write_all(&bytes).await?;
    writer.flush().await
}

pub async fn read_frame<R: AsyncRead + Unpin, T: Frame>(reader: &mut R) -> io::Result<T> {
    let kind = reader.read_u8().await?;
    let length = reader.read_u32().await? as usize;
    if length > MAX_FRAME_BYTES {
        return Err(invalid());
    }
    let mut bytes = Zeroizing::new(vec![0; length]);
    reader.read_exact(&mut bytes).await?;
    if kind == 0 {
        serde_json::from_slice(&bytes).map_err(|_| invalid())
    } else {
        T::from_raw(kind, &bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn raw_input_has_a_fixed_binary_contract_and_rejects_oversized_frames() {
        let request = Request::Input {
            prediction_id: 7,
            bytes: Zeroizing::new(vec![0, 0xff, 0x1b]),
        };
        let mut encoded = Vec::new();
        write_frame(&mut encoded, &request).await.unwrap();
        assert_eq!(
            encoded,
            [1, 0, 0, 0, 11, 0, 0, 0, 0, 0, 0, 0, 7, 0, 0xff, 0x1b]
        );
        let oversized = [2, 0, 1, 0, 1];
        assert_eq!(
            read_frame::<_, Event>(&mut oversized.as_slice())
                .await
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
        let truncated = [1, 0, 0, 0, 2, 1, 2];
        assert_eq!(
            read_frame::<_, Request>(&mut truncated.as_slice())
                .await
                .err()
                .unwrap()
                .kind(),
            io::ErrorKind::InvalidData
        );
    }
}
