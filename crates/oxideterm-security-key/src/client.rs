// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::*;
use serde::{Serialize, de::DeserializeOwned};
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    process::Stdio,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    process::Command,
};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

pub trait SecurityKeyInteraction: Send + Sync {
    fn error_message(
        &self,
        error: SecurityKeyError,
    ) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(async move { error.to_string() })
    }
    fn pin(
        &self,
        retry: bool,
    ) -> Pin<Box<dyn Future<Output = Result<Zeroizing<String>, SecurityKeyError>> + Send + '_>>;
    /// Remains pending while the device waits; cancellation must end the signing attempt.
    fn touch(&self) -> Pin<Box<dyn Future<Output = Result<(), SecurityKeyError>> + Send + '_>>;
}

struct State {
    retired: bool,
    active: usize,
}
pub struct SecurityKeyProvider {
    executable: PathBuf,
    state: Mutex<State>,
    idle: Condvar,
    shutdown: CancellationToken,
}

impl SecurityKeyProvider {
    pub fn new(executable: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            executable,
            state: Mutex::new(State {
                retired: false,
                active: 0,
            }),
            idle: Condvar::new(),
            shutdown: CancellationToken::new(),
        })
    }

    pub fn executable(&self) -> &std::path::Path {
        &self.executable
    }

    pub fn retire(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        let provider = Arc::clone(self);
        {
            let mut state = provider.state.lock().unwrap_or_else(|e| e.into_inner());
            state.retired = true;
            provider.shutdown.cancel();
        }
        std::thread::spawn(move || {
            let mut state = provider.state.lock().unwrap_or_else(|e| e.into_inner());
            while state.active != 0 {
                state = provider.idle.wait(state).unwrap_or_else(|e| e.into_inner());
            }
        })
    }

    pub async fn sign(
        self: &Arc<Self>,
        request: SecurityKeyRequest,
        interaction: Arc<dyn SecurityKeyInteraction>,
    ) -> Result<SecurityKeyResponse, SecurityKeyError> {
        if !matches!(&request, SecurityKeyRequest::Sign { .. }) {
            return Err(SecurityKeyError::InvalidRequest);
        }
        let lease = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.retired {
                return Err(SecurityKeyError::Unavailable);
            }
            state.active += 1;
            Lease(Arc::clone(self))
        };
        let cancel = CancellationToken::new();
        let _cancel_on_drop = CancelOnDrop(cancel.clone());
        let (sender, receiver) = tokio::sync::oneshot::channel();
        // The worker owns the child until wait() completes. Dropping the caller
        // cancels this worker without abandoning the process or its secret input.
        tokio::spawn(async move {
            let provider = Arc::clone(&lease.0);
            let result = run_sign(&provider, request, interaction, cancel).await;
            drop(lease);
            let _ = sender.send(result);
        });
        receiver
            .await
            .map_err(|_| SecurityKeyError::DeviceFailure)?
    }
}

struct Lease(Arc<SecurityKeyProvider>);
impl Drop for Lease {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        self.0.idle.notify_all();
    }
}
struct CancelOnDrop(CancellationToken);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

async fn run_sign(
    provider: &SecurityKeyProvider,
    request: SecurityKeyRequest,
    interaction: Arc<dyn SecurityKeyInteraction>,
    cancel: CancellationToken,
) -> Result<SecurityKeyResponse, SecurityKeyError> {
    let mut command = Command::new(&provider.executable);
    // The signer needs device access, not the application's model tokens or
    // agent environment. Bundled libraries resolve relative to its executable.
    command.env_clear();
    for name in [
        "SystemRoot",
        "WINDIR",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "TMPDIR",
        "TEMP",
        "TMP",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|_| SecurityKeyError::Unavailable)?;
    let result = tokio::select! {
        _ = cancel.cancelled() => Err(SecurityKeyError::Cancelled),
        _ = provider.shutdown.cancelled() => Err(SecurityKeyError::Cancelled),
        result = tokio::time::timeout(Duration::from_secs(60), async {
            let mut input = child.stdin.take().ok_or(SecurityKeyError::DeviceFailure)?;
            let mut output = child.stdout.take().ok_or(SecurityKeyError::DeviceFailure)?;
            match read_async::<SecurityKeyResponse>(&mut output).await? {
                SecurityKeyResponse::Ready { protocol_version: SECURITY_KEY_PROTOCOL_VERSION } => {},
                _ => return Err(SecurityKeyError::Incompatible),
            }
            write_async(&mut input, &request).await?;
            drop(request);
            let mut touch = None;
            loop {
                tokio::select! {
                    answer = read_async::<SecurityKeyResponse>(&mut output) => match answer? {
                        SecurityKeyResponse::Touch => { touch = Some(interaction.touch()); },
                        SecurityKeyResponse::PinRequired { retry } => {
                            touch = None;
                            let value = interaction.pin(retry).await?;
                            write_async(&mut input, &SecurityKeyRequest::Pin { value }).await?;
                        },
                        answer @ SecurityKeyResponse::Signature { .. } => return Ok(answer),
                        SecurityKeyResponse::Failure { code } => return Err(code),
                        SecurityKeyResponse::Ready { .. } => return Err(SecurityKeyError::Incompatible),
                    },
                    result = async { match touch.as_mut() { Some(waiter) => waiter.await, None => std::future::pending().await } } => {
                        return Err(result.err().unwrap_or(SecurityKeyError::Cancelled));
                    }
                }
            }
        }) => result.unwrap_or(Err(SecurityKeyError::Timeout)),
    };
    // Even success ends this single-operation process. Reap before releasing
    // the lease so package replacement cannot race a Windows executable lock.
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

async fn read_async<T: DeserializeOwned>(
    reader: &mut (impl AsyncRead + Unpin),
) -> Result<T, SecurityKeyError> {
    let length = reader
        .read_u32()
        .await
        .map_err(|_| SecurityKeyError::DeviceFailure)? as usize;
    if length == 0 || length > SECURITY_KEY_MAX_FRAME_BYTES {
        return Err(SecurityKeyError::Incompatible);
    }
    let mut bytes = Zeroizing::new(vec![0; length]);
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|_| SecurityKeyError::DeviceFailure)?;
    serde_json::from_slice(&bytes).map_err(|_| SecurityKeyError::Incompatible)
}

async fn write_async(
    writer: &mut (impl AsyncWrite + Unpin),
    value: &impl Serialize,
) -> Result<(), SecurityKeyError> {
    let bytes = encode_message(value).map_err(|_| SecurityKeyError::InvalidRequest)?;
    writer
        .write_u32(bytes.len() as u32)
        .await
        .map_err(|_| SecurityKeyError::DeviceFailure)?;
    writer
        .write_all(&bytes)
        .await
        .map_err(|_| SecurityKeyError::DeviceFailure)?;
    writer
        .flush()
        .await
        .map_err(|_| SecurityKeyError::DeviceFailure)
}
