// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    MoshIpFamily,
    wire::{self, Event, Request},
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};
use tokio::{
    process::Command,
    sync::{mpsc, watch},
    task::JoinHandle,
};
use zeroize::Zeroize;

pub struct MoshSessionConfig {
    pub remote_host: String,
    pub remote_port: u16,
    pub ip_family: MoshIpFamily,
    pub columns: u16,
    pub rows: u16,
    pub key: crate::MoshSessionKey,
    pub executable: PathBuf,
    pub lease: MoshSessionLease,
}

pub enum MoshSessionEvent {
    Output(Vec<u8>),
    RemoteResize { columns: u16, rows: u16 },
    ConnectionStateChanged(crate::MoshConnectionState),
    RoundTripEstimate(u16),
    PredictionAcknowledged(u64),
    RemoteStateAdvanced(u64),
    Closed(crate::ShutdownOutcome),
    Failed(String),
}

#[derive(Default)]
struct Sessions {
    blocked: bool,
    next_id: u64,
    active: HashMap<u64, watch::Sender<bool>>,
}

struct QueuedEvent(MoshSessionEvent);
impl QueuedEvent {
    fn into_event(mut self) -> MoshSessionEvent {
        std::mem::replace(&mut self.0, MoshSessionEvent::RemoteStateAdvanced(0))
    }
}
impl Drop for QueuedEvent {
    fn drop(&mut self) {
        if let MoshSessionEvent::Output(bytes) = &mut self.0 {
            bytes.zeroize();
        }
    }
}

/// The workspace can stop and reap every engine before replacing its package.
#[derive(Default)]
pub struct MoshPluginSessions {
    state: Mutex<Sessions>,
    completed: Condvar,
}

impl MoshPluginSessions {
    pub fn set_available(&self, available: bool) {
        let mut state = self.state.lock().unwrap();
        state.blocked = !available;
        if !available {
            for cancel in state.active.values() {
                let _ = cancel.send(true);
            }
        }
    }

    pub fn stop(self: &Arc<Self>) -> std::thread::JoinHandle<()> {
        self.set_available(false);
        let sessions = self.clone();
        std::thread::spawn(move || {
            let mut state = sessions.state.lock().unwrap();
            while !state.active.is_empty() {
                state = sessions.completed.wait(state).unwrap();
            }
        })
    }

    pub fn acquire(self: &Arc<Self>) -> Result<MoshSessionLease, MoshSessionStartError> {
        let mut state = self.state.lock().unwrap();
        if state.blocked {
            return Err(MoshSessionStartError::Unavailable);
        }
        state.next_id += 1;
        let id = state.next_id;
        let (cancel, receiver) = watch::channel(false);
        state.active.insert(id, cancel);
        Ok(MoshSessionLease {
            registration: Registration {
                id,
                sessions: self.clone(),
            },
            cancellation: receiver,
        })
    }
}

/// Acquired before SSH bootstrap so retired startup work cannot relaunch an engine.
pub struct MoshSessionLease {
    registration: Registration,
    cancellation: watch::Receiver<bool>,
}

impl MoshSessionLease {
    pub fn cancellation(&self) -> MoshSessionCancellation {
        MoshSessionCancellation(
            self.registration.sessions.state.lock().unwrap().active[&self.registration.id].clone(),
        )
    }

    pub async fn cancelled(&mut self) {
        if !*self.cancellation.borrow() {
            let _ = self.cancellation.changed().await;
        }
    }
}

pub struct MoshSessionCancellation(watch::Sender<bool>);
impl MoshSessionCancellation {
    pub fn cancel(&self) {
        let _ = self.0.send(true);
    }
}

struct Registration {
    id: u64,
    sessions: Arc<MoshPluginSessions>,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.sessions.state.lock().unwrap().active.remove(&self.id);
        self.sessions.completed.notify_all();
    }
}

pub struct MoshSessionClient {
    command_tx: mpsc::Sender<Request>,
    event_rx: mpsc::Receiver<QueuedEvent>,
}
impl MoshSessionClient {
    pub async fn send_input_for_prediction(
        &self,
        prediction_id: u64,
        bytes: Vec<u8>,
    ) -> Result<(), MoshSessionCommandError> {
        let bytes = zeroize::Zeroizing::new(bytes);
        for chunk in bytes.chunks(wire::MAX_FRAME_BYTES - 8) {
            self.command_tx
                .send(Request::Input {
                    prediction_id,
                    bytes: zeroize::Zeroizing::new(chunk.to_vec()),
                })
                .await
                .map_err(|_| MoshSessionCommandError::Closed)?;
        }
        Ok(())
    }
    pub async fn resize(&self, columns: u16, rows: u16) -> Result<(), MoshSessionCommandError> {
        self.command_tx
            .send(Request::Resize { columns, rows })
            .await
            .map_err(|_| MoshSessionCommandError::Closed)
    }
    pub async fn next_event(&mut self) -> Option<MoshSessionEvent> {
        self.event_rx.recv().await.map(QueuedEvent::into_event)
    }
    pub fn try_next_event(&mut self) -> Option<MoshSessionEvent> {
        self.event_rx.try_recv().ok().map(QueuedEvent::into_event)
    }
}

pub struct MoshSessionOwner {
    command_tx: mpsc::Sender<Request>,
    cancel: watch::Sender<bool>,
    task: Option<JoinHandle<()>>,
}
impl MoshSessionOwner {
    pub async fn shutdown(mut self) {
        if let Some(mut task) = self.task.take() {
            // Include a congested input queue in the graceful-shutdown deadline.
            let graceful = async {
                let _ = self.command_tx.send(Request::Shutdown).await;
                let _ = (&mut task).await;
            };
            if tokio::time::timeout(Duration::from_secs(12), graceful)
                .await
                .is_err()
            {
                self.cancel();
                let _ = task.await;
            }
        }
    }
    pub fn cancel(&mut self) {
        let _ = self.cancel.send(true);
    }
}
impl Drop for MoshSessionOwner {
    fn drop(&mut self) {
        self.cancel();
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MoshSessionStartError {
    #[error("Mosh UDP endpoint is invalid")]
    InvalidEndpoint,
    #[error("Mosh plugin is unavailable")]
    Unavailable,
    #[error("Mosh plugin could not be started")]
    Launch,
    #[error("Mosh plugin protocol is unsupported or failed")]
    Protocol,
}
#[derive(Clone, Copy, Debug, thiserror::Error, Eq, PartialEq)]
pub enum MoshSessionCommandError {
    #[error("Mosh session is closed")]
    Closed,
}

pub async fn start_mosh_session(
    config: MoshSessionConfig,
) -> Result<(MoshSessionClient, MoshSessionOwner), MoshSessionStartError> {
    if config.remote_port == 0 || config.columns == 0 || config.rows == 0 {
        return Err(MoshSessionStartError::InvalidEndpoint);
    }
    let MoshSessionLease {
        registration,
        cancellation: mut registry_cancel,
    } = config.lease;
    if *registry_cancel.borrow() {
        return Err(MoshSessionStartError::Unavailable);
    }
    let mut command = Command::new(&config.executable);
    command
        .arg("--stdio")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let mut child = command.spawn().map_err(|_| MoshSessionStartError::Launch)?;
    let mut stdin = child.stdin.take().ok_or(MoshSessionStartError::Launch)?;
    let mut stdout = child.stdout.take().ok_or(MoshSessionStartError::Launch)?;
    let start = Request::Start {
        version: wire::PROTOCOL_VERSION,
        host: config.remote_host,
        port: config.remote_port,
        family: config.ip_family,
        columns: config.columns,
        rows: config.rows,
        key: config.key.into_encoded(),
    };
    let handshake = async {
        wire::write_frame(&mut stdin, &start).await?;
        match wire::read_frame::<_, Event>(&mut stdout).await? {
            Event::Ready {
                version: wire::PROTOCOL_VERSION,
            } => Ok(()),
            _ => Err(std::io::Error::other("Mosh plugin handshake failed")),
        }
    };
    let result = tokio::select! {
        _ = registry_cancel.changed() => Err(MoshSessionStartError::Unavailable),
        result = tokio::time::timeout(Duration::from_secs(10), handshake) => result.map_err(|_| MoshSessionStartError::Protocol).and_then(|result| result.map_err(|_| MoshSessionStartError::Protocol)),
    };
    drop(start);
    if let Err(error) = result {
        let _ = child.kill().await;
        let _ = child.wait().await;
        return Err(error);
    }
    let (command_tx, mut commands) = mpsc::channel::<Request>(64);
    // 16 frames capped at 64 KiB bound the queued terminal output to 1 MiB.
    let (event_tx, event_rx) = mpsc::channel(16);
    let (cancel, mut cancelled) = watch::channel(false);
    let task = tokio::spawn(async move {
        let _registration = registration;
        let result = tokio::select! {
            _ = cancelled.changed() => Ok(()),
            _ = registry_cancel.changed() => Ok(()),
            result = relay(&mut stdout, &mut stdin, &mut commands, &event_tx) => result,
        };
        // Reap before notifying package management that its executable can be replaced.
        let _ = child.kill().await;
        let _ = child.wait().await;
        if result.is_err() {
            let _ = event_tx.try_send(QueuedEvent(MoshSessionEvent::Failed(
                "Mosh plugin transport failed".into(),
            )));
        }
    });
    Ok((
        MoshSessionClient {
            command_tx: command_tx.clone(),
            event_rx,
        },
        MoshSessionOwner {
            command_tx,
            cancel,
            task: Some(task),
        },
    ))
}

async fn relay(
    reader: &mut (impl tokio::io::AsyncRead + Unpin),
    writer: &mut (impl tokio::io::AsyncWrite + Unpin),
    commands: &mut mpsc::Receiver<Request>,
    event_tx: &mpsc::Sender<QueuedEvent>,
) -> std::io::Result<()> {
    let requests = async {
        while let Some(request) = commands.recv().await {
            wire::write_frame(writer, &request).await?;
        }
        Ok::<(), std::io::Error>(())
    };
    let events = async {
        loop {
            let event = wire::read_frame::<_, Event>(reader).await?;
            let event = match event {
                Event::Ready { .. } | Event::Failed => {
                    return Err(std::io::Error::other("Mosh engine failed"));
                }
                Event::Output(bytes) => MoshSessionEvent::Output(bytes.to_vec()),
                Event::RemoteResize { columns, rows } => {
                    MoshSessionEvent::RemoteResize { columns, rows }
                }
                Event::ConnectionStateChanged(state) => {
                    MoshSessionEvent::ConnectionStateChanged(state)
                }
                Event::RoundTripEstimate(value) => MoshSessionEvent::RoundTripEstimate(value),
                Event::PredictionAcknowledged(value) => {
                    MoshSessionEvent::PredictionAcknowledged(value)
                }
                Event::RemoteStateAdvanced(value) => MoshSessionEvent::RemoteStateAdvanced(value),
                Event::Closed(outcome) => {
                    let _ = event_tx
                        .send(QueuedEvent(MoshSessionEvent::Closed(outcome)))
                        .await;
                    return Ok(());
                }
            };
            if event_tx.send(QueuedEvent(event)).await.is_err() {
                return Ok(());
            }
        }
    };
    // Rendering backpressure is independent from terminal input and shutdown.
    tokio::select! {
        result = requests => result,
        result = events => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn retired_startup_cannot_relaunch_after_the_provider_becomes_available() {
        let sessions = Arc::new(MoshPluginSessions::default());
        let mut lease = sessions.acquire().unwrap();
        let retirement = sessions.stop();
        assert!(matches!(
            sessions.acquire(),
            Err(MoshSessionStartError::Unavailable)
        ));
        sessions.set_available(true);
        tokio::time::timeout(Duration::from_secs(1), lease.cancelled())
            .await
            .unwrap();
        let result = start_mosh_session(MoshSessionConfig {
            remote_host: "127.0.0.1".into(),
            remote_port: 60001,
            ip_family: MoshIpFamily::Ipv4,
            columns: 80,
            rows: 24,
            key: crate::MoshSessionKey::decode("AQIDBAUGBwgJCgsMDQ4PEA").unwrap(),
            executable: PathBuf::new(),
            lease,
        })
        .await;
        assert!(matches!(result, Err(MoshSessionStartError::Unavailable)));
        retirement.join().unwrap();

        let mut closed_pane = sessions.acquire().unwrap();
        closed_pane.cancellation().cancel();
        tokio::time::timeout(Duration::from_secs(1), closed_pane.cancelled())
            .await
            .unwrap();
        let healthy = sessions.acquire().unwrap();
        assert!(!*healthy.cancellation.borrow());
    }

    #[tokio::test]
    async fn unread_terminal_output_does_not_block_pipe_input_or_retirement() {
        let (host, engine) = tokio::io::duplex(1);
        let (mut stdout, mut stdin) = tokio::io::split(host);
        let (mut input, mut display) = tokio::io::split(engine);
        let (commands, mut command_rx) = mpsc::channel(1);
        let (events, mut event_rx) = mpsc::channel(1);
        let task =
            tokio::spawn(
                async move { relay(&mut stdout, &mut stdin, &mut command_rx, &events).await },
            );
        wire::write_frame(
            &mut display,
            &Event::Output(zeroize::Zeroizing::new(b"first".to_vec())),
        )
        .await
        .unwrap();
        wire::write_frame(
            &mut display,
            &Event::Output(zeroize::Zeroizing::new(b"blocked".to_vec())),
        )
        .await
        .unwrap();
        // A third frame header fits only after the second frame was consumed;
        // its delivery is blocked by the unread, capacity-one terminal mailbox.
        display.write_u8(2).await.unwrap();
        commands
            .send(Request::Input {
                prediction_id: 42,
                bytes: zeroize::Zeroizing::new(b"input-under-pressure".to_vec()),
            })
            .await
            .unwrap();
        let received = tokio::time::timeout(
            Duration::from_secs(3),
            wire::read_frame::<_, Request>(&mut input),
        )
        .await
        .expect("unread output blocked the stdin pipe")
        .unwrap();
        match received {
            Request::Input {
                prediction_id,
                bytes,
            } => {
                assert_eq!(prediction_id, 42);
                assert_eq!(bytes.as_slice(), b"input-under-pressure");
            }
            _ => panic!("expected input frame"),
        }
        drop(commands);
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .expect("unread output blocked retirement")
            .unwrap()
            .unwrap();
        match event_rx.recv().await.unwrap().into_event() {
            MoshSessionEvent::Output(bytes) => assert_eq!(bytes, b"first"),
            _ => panic!("expected first output"),
        }
    }
}
