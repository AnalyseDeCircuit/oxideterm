// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io;

use thiserror::Error;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{
    X11AuthMaterial, X11ForwardingError, X11LocalEndpoint, X11SetupBuffer, X11SetupDecision,
};

const X11_RUNTIME_READ_BUFFER_BYTES: usize = 32 * 1024;

pub trait X11AsyncStream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> X11AsyncStream for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

pub type BoxedX11Stream = Box<dyn X11AsyncStream>;

#[derive(Debug, Error)]
pub enum X11RuntimeError {
    #[error(transparent)]
    Forwarding(#[from] X11ForwardingError),
    #[error("X11 I/O failed: {0}")]
    Io(String),
    #[error("X11 endpoint is unsupported on this platform: {0}")]
    UnsupportedEndpoint(&'static str),
}

impl From<io::Error> for X11RuntimeError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

pub async fn connect_local_x11_endpoint(
    endpoint: &X11LocalEndpoint,
) -> Result<BoxedX11Stream, X11RuntimeError> {
    match endpoint {
        X11LocalEndpoint::Tcp { host, port } => {
            let stream = tokio::net::TcpStream::connect((host.as_str(), *port)).await?;
            Ok(Box::new(stream))
        }
        X11LocalEndpoint::UnixSocket { path } => connect_unix_socket(path).await,
    }
}

pub async fn bridge_x11_stream_to_endpoint<S>(
    mut ssh_stream: S,
    endpoint: &X11LocalEndpoint,
    auth: &X11AuthMaterial,
) -> Result<(), X11RuntimeError>
where
    S: X11AsyncStream + 'static,
{
    let mut setup = X11SetupBuffer::new();
    let mut read_buffer = vec![0u8; X11_RUNTIME_READ_BUFFER_BYTES];

    loop {
        let read = ssh_stream.read(&mut read_buffer).await?;
        if read == 0 {
            return Err(X11RuntimeError::Io(
                "X11 channel closed before setup packet completed".to_string(),
            ));
        }

        let Some(decision) = setup.push_decision(&read_buffer[..read], auth)? else {
            continue;
        };

        match decision {
            X11SetupDecision::Forward(rewrite) => {
                let mut local_stream = connect_local_x11_endpoint(endpoint).await?;
                local_stream.write_all(&rewrite.rewritten_setup).await?;
                if !rewrite.trailing_data.is_empty() {
                    local_stream.write_all(&rewrite.trailing_data).await?;
                }
                tokio::io::copy_bidirectional(&mut ssh_stream, &mut local_stream).await?;
                return Ok(());
            }
            X11SetupDecision::Reject(reject) => {
                ssh_stream.write_all(&reject.failure_response).await?;
                let _ = ssh_stream.shutdown().await;
                return Ok(());
            }
        }
    }
}

#[cfg(unix)]
async fn connect_unix_socket(path: &str) -> Result<BoxedX11Stream, X11RuntimeError> {
    let stream = tokio::net::UnixStream::connect(path).await?;
    Ok(Box::new(stream))
}

#[cfg(not(unix))]
async fn connect_unix_socket(_path: &str) -> Result<BoxedX11Stream, X11RuntimeError> {
    Err(X11RuntimeError::UnsupportedEndpoint(
        "Unix X11 sockets require a Unix-like host",
    ))
}
