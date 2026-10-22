// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_ssh::SshTransportCommand;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::{mpsc, oneshot, watch};
use zeroize::Zeroizing;

pub type ShellChannelAcquisition = oneshot::Receiver<anyhow::Result<ShellChannel>>;

/// An exclusive lease on one existing PTY, never on its shared SSH connection.
pub struct ShellChannel {
    pub token: String,
    writer: mpsc::Sender<SshTransportCommand>,
    pub output: mpsc::Receiver<Zeroizing<Vec<u8>>>,
    restored: Arc<AtomicBool>,
    released: watch::Sender<bool>,
    started: AtomicBool,
    audit: Option<oxideterm_audit::AuditContext>,
    failure_handler: Option<Arc<dyn Fn() + Send + Sync>>,
    release_handler: Option<Arc<dyn Fn() + Send + Sync>>,
    _terminal: Option<Arc<parking_lot::Mutex<crate::TerminalSession>>>,
}

impl ShellChannel {
    pub fn audit_context(&self) -> Option<oxideterm_audit::AuditContext> {
        self.audit.clone()
    }

    pub(crate) fn set_audit_context(&mut self, context: Option<oxideterm_audit::AuditContext>) {
        self.audit = context;
    }

    pub fn retain_terminal(&mut self, terminal: Arc<parking_lot::Mutex<crate::TerminalSession>>) {
        self._terminal = Some(terminal);
    }
    pub(crate) fn set_failure_handler(&mut self, handler: Arc<dyn Fn() + Send + Sync>) {
        self.failure_handler = Some(handler);
    }

    pub(crate) fn set_release_handler(&mut self, handler: Arc<dyn Fn() + Send + Sync>) {
        self.release_handler = Some(handler);
    }

    fn release_unstarted(&self) {
        self.restored.store(true, Ordering::Release);
        self.released.send_replace(true);
        if let Some(handler) = &self.release_handler {
            handler();
        }
    }

    pub async fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        self.write_with_timeout(bytes, std::time::Duration::from_secs(5))
            .await
    }

    pub async fn write_with_timeout(
        &self,
        bytes: &[u8],
        timeout: std::time::Duration,
    ) -> anyhow::Result<()> {
        self.started.store(true, Ordering::Release);
        for chunk in bytes.chunks(4096) {
            tokio::time::timeout(
                timeout,
                self.writer.send(SshTransportCommand::Data(chunk.to_vec())),
            )
            .await
            .map_err(|_| anyhow::anyhow!("Shell channel write timed out"))?
            .map_err(|_| anyhow::anyhow!("Shell channel closed"))?;
        }
        Ok(())
    }

    pub fn is_restored(&self) -> bool {
        self.restored.load(Ordering::Acquire)
    }

    /// Completes when the original PTY is restored, or closed after recovery fails.
    pub fn release_receiver(&self) -> watch::Receiver<bool> {
        self.released.subscribe()
    }

    pub async fn restore(&mut self) -> anyhow::Result<()> {
        if !self.started.load(Ordering::Acquire) {
            self.release_unstarted();
            return Ok(());
        }
        restore_shell(&self.writer, &mut self.output, &self.restored, &self.token).await
    }
}

async fn restore_shell(
    writer: &mpsc::Sender<SshTransportCommand>,
    output: &mut mpsc::Receiver<Zeroizing<Vec<u8>>>,
    restored: &AtomicBool,
    token: &str,
) -> anyhow::Result<()> {
    if restored.load(Ordering::Acquire) {
        return Ok(());
    }
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        // Every bootstrap reader recognizes this line. If the child has already
        // exited, it is only a comment in the original shell, never a command or signal.
        writer
            .send(SshTransportCommand::Data(
                format!("\n#oxideterm-cancel-{token}\n").into_bytes(),
            ))
            .await
            .map_err(|_| anyhow::anyhow!("Shell channel closed during restoration"))?;
        while !restored.load(Ordering::Acquire) {
            if output.recv().await.is_none() && !restored.load(Ordering::Acquire) {
                anyhow::bail!("Shell restoration was not acknowledged");
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| anyhow::anyhow!("Shell restoration timed out"))?
}

impl Drop for ShellChannel {
    fn drop(&mut self) {
        if !self.started.load(Ordering::Acquire) {
            // An abandoned acquisition has not changed the remote shell at all.
            self.release_unstarted();
        }
        if !self.is_restored() {
            if let Ok(runtime) = tokio::runtime::Handle::try_current() {
                let writer = self.writer.clone();
                let mut output = std::mem::replace(&mut self.output, mpsc::channel(1).1);
                let restored = self.restored.clone();
                let token = self.token.clone();
                let handler = self.failure_handler.take();
                let terminal = self._terminal.take();
                // Cancellation transfers the exact PTY lease to this bounded cleanup task.
                // The original terminal remains alive until its restoration is acknowledged.
                runtime.spawn(async move {
                    let _terminal = terminal;
                    if restore_shell(&writer, &mut output, &restored, &token)
                        .await
                        .is_err()
                    {
                        if let Some(handler) = handler {
                            handler();
                        } else {
                            let _ = writer.try_send(SshTransportCommand::Close);
                        }
                    }
                });
                return;
            }
            if let Some(handler) = &self.failure_handler {
                handler();
            } else {
                let _ = self.writer.try_send(SshTransportCommand::Close);
            }
        }
    }
}

pub(crate) struct ShellChannelSink {
    output: Option<mpsc::Sender<Zeroizing<Vec<u8>>>>,
    marker: Vec<u8>,
    tail: Zeroizing<Vec<u8>>,
    restored: Arc<AtomicBool>,
    released: watch::Sender<bool>,
}

impl Drop for ShellChannelSink {
    fn drop(&mut self) {
        // Closing this exact PTY also ends the wait; it must not wait on shared node lifetime.
        self.released.send_replace(true);
    }
}

pub(crate) fn channel(
    writer: mpsc::Sender<SshTransportCommand>,
) -> (ShellChannel, ShellChannelSink) {
    let token = uuid::Uuid::new_v4().simple().to_string();
    let marker = format!("\x1e{token}:RESTORED\x1f").into_bytes();
    let (tx, rx) = mpsc::channel(4096);
    let restored = Arc::new(AtomicBool::new(false));
    let (released, _) = watch::channel(false);
    (
        ShellChannel {
            token,
            writer,
            output: rx,
            restored: restored.clone(),
            released: released.clone(),
            started: AtomicBool::new(false),
            audit: None,
            failure_handler: None,
            release_handler: None,
            _terminal: None,
        },
        ShellChannelSink {
            output: Some(tx),
            marker,
            tail: Zeroizing::new(Vec::new()),
            restored,
            released,
        },
    )
}

impl ShellChannelSink {
    pub(crate) fn is_active(&self) -> bool {
        !self.restored.load(Ordering::Acquire)
    }

    pub(crate) fn feed(&mut self, bytes: &[u8]) -> Vec<u8> {
        if !self.is_active() {
            return bytes.to_vec();
        }
        self.tail.extend_from_slice(bytes);
        if let Some(index) = memchr::memmem::find(&self.tail, &self.marker) {
            let visible = self.tail[index + self.marker.len()..].to_vec();
            zeroize::Zeroize::zeroize(&mut *self.tail);
            self.restored.store(true, Ordering::Release);
            self.output.take();
            self.released.send_replace(true);
            return visible;
        }
        // Keep only a possible split restoration marker; all protocol bytes stay private.
        let retained = (1..self.marker.len())
            .rev()
            .find(|&n| self.tail.ends_with(&self.marker[..n]))
            .unwrap_or(0);
        let count = self.tail.len() - retained;
        if count > 0 {
            let chunk = Zeroizing::new(self.tail[..count].to_vec());
            if let Some(sender) = &self.output {
                if sender.try_send(chunk).is_err() {
                    self.output.take();
                }
            }
            zeroize::Zeroize::zeroize(&mut self.tail[..count]);
            self.tail.drain(..count);
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn abandoned_started_lease_restores_before_releasing_exclusivity() {
        let (writer, mut input) = mpsc::channel(8);
        let (mut lease, mut sink) = channel(writer.clone());
        let token = lease.token.clone();
        let failed = Arc::new(AtomicBool::new(false));
        let failure = failed.clone();
        lease.set_failure_handler(Arc::new(move || failure.store(true, Ordering::Release)));
        lease.write(b"bootstrap\r").await.unwrap();
        assert!(
            matches!(input.recv().await, Some(SshTransportCommand::Data(bytes)) if bytes == b"bootstrap\r")
        );
        let mut released = lease.release_receiver();
        let notification = tokio::spawn(async move {
            let _ = released.wait_for(|released| *released).await;
        });
        drop(lease);
        let cancel = tokio::time::timeout(std::time::Duration::from_secs(1), input.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            matches!(cancel, SshTransportCommand::Data(bytes) if bytes == format!("\n#oxideterm-cancel-{token}\n").as_bytes())
        );
        assert!(sink.is_active());
        assert!(
            !notification.is_finished(),
            "failure must wait for the original PTY"
        );
        assert_eq!(sink.feed(format!("\x1e{token}:RESTO").as_bytes()), b"");
        tokio::task::yield_now().await;
        assert!(
            !notification.is_finished(),
            "a partial recovery marker is not confirmation"
        );
        assert_eq!(sink.feed(b"RED\x1foriginal# "), b"original# ");
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            while writer.strong_count() > 1 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(1), notification)
            .await
            .unwrap()
            .unwrap();
        assert!(!sink.is_active());
        assert!(!failed.load(Ordering::Acquire));
        assert!(matches!(
            input.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
    }

    #[tokio::test]
    async fn unused_lease_and_closed_terminal_complete_release_notifications() {
        let (writer, mut input) = mpsc::channel(8);
        let (lease, sink) = channel(writer.clone());
        let mut released = lease.release_receiver();
        drop(lease);
        released.wait_for(|released| *released).await.unwrap();
        assert!(!sink.is_active());
        assert!(matches!(
            input.try_recv(),
            Err(mpsc::error::TryRecvError::Empty)
        ));
        drop(sink);

        let (lease, sink) = channel(writer);
        lease.write(b"bootstrap\r").await.unwrap();
        assert!(
            matches!(input.recv().await, Some(SshTransportCommand::Data(bytes)) if bytes == b"bootstrap\r")
        );
        let mut released = lease.release_receiver();
        drop(sink);
        released.wait_for(|released| *released).await.unwrap();
        assert!(
            !lease.is_restored(),
            "closing a PTY must not claim interactive recovery"
        );
        drop(lease);
    }

    #[test]
    fn protocol_is_private_and_split_restoration_releases_only_shell_output() {
        let (tx, _rx) = mpsc::channel(8);
        let (mut lease, mut sink) = channel(tx);
        let marker = format!("\x1e{}:RESTORED\x1f", lease.token);
        assert_eq!(sink.feed(b"secret file payload and upload echo"), b"");
        assert_eq!(sink.feed(&marker.as_bytes()[..10]), b"");
        assert!(!lease.is_restored());
        let mut end = marker.as_bytes()[10..].to_vec();
        end.extend_from_slice(b"root@container# ");
        assert_eq!(sink.feed(&end), b"root@container# ");
        assert!(lease.is_restored());
        assert_eq!(sink.feed(b"ls\r\n"), b"ls\r\n");
        let private = lease.output.try_recv().unwrap();
        assert_eq!(&*private, b"secret file payload and upload echo");
    }
}
