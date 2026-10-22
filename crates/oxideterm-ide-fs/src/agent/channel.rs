// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use zeroize::Zeroizing;

/// Only the selected existing terminal has restoration duties toward an interactive parent.
pub(super) enum IdeChannel {
    Terminal(oxideterm_terminal::ShellChannel),
    Dedicated(DedicatedIdeChannel),
}

pub(super) struct DedicatedIdeChannel {
    channel: Option<russh::Channel<russh::client::Msg>>,
    token: String,
    pty: bool,
    exec: bool,
    restored: bool,
    tail: Zeroizing<Vec<u8>>,
    _owner: Arc<BoundNodeOwner>,
}

impl Drop for DedicatedIdeChannel {
    fn drop(&mut self) {
        if let Some(channel) = self.channel.take() {
            let owner = self._owner.clone();
            // Cleanup retains only this IDE consumer until its channel has closed.
            tokio::spawn(async move {
                let _owner = owner;
                let _ = tokio::time::timeout(Duration::from_secs(5), channel.close()).await;
            });
        }
    }
}

impl IdeChannel {
    pub(super) fn token(&self) -> &str {
        match self {
            Self::Terminal(c) => &c.token,
            Self::Dedicated(c) => &c.token,
        }
    }
    pub(super) fn with_pty(&self) -> bool {
        match self {
            Self::Terminal(_) => true,
            Self::Dedicated(c) => c.pty,
        }
    }
    pub(super) fn is_ssh_shell(&self) -> bool {
        matches!(self, Self::Dedicated(c) if !c.exec)
    }
    pub(super) async fn write(&self, bytes: &[u8]) -> anyhow::Result<()> {
        self.write_with_timeout(bytes, Duration::from_secs(5)).await
    }

    pub(super) async fn write_with_timeout(
        &self,
        bytes: &[u8],
        timeout: Duration,
    ) -> anyhow::Result<()> {
        match self {
            Self::Terminal(c) => c.write_with_timeout(bytes, timeout).await,
            Self::Dedicated(c) => {
                let channel = c
                    .channel
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("IDE channel closed"))?;
                for chunk in bytes.chunks(4096) {
                    tokio::time::timeout(timeout, channel.data(chunk)).await??;
                }
                Ok(())
            }
        }
    }
    pub(super) async fn recv(&mut self) -> Option<Zeroizing<Vec<u8>>> {
        match self {
            Self::Terminal(c) => c.output.recv().await,
            Self::Dedicated(c) => {
                let marker = format!("\x1e{}:RESTORED\x1f", c.token);
                loop {
                    if c.restored {
                        return None;
                    }
                    match c.channel.as_mut()?.wait().await? {
                        ChannelMsg::Data { data } => {
                            c.tail.extend_from_slice(&data);
                            if memchr::memmem::find(&c.tail, marker.as_bytes()).is_some() {
                                c.restored = true;
                                zeroize::Zeroize::zeroize(&mut *c.tail);
                                return None;
                            }
                            let retained = (1..marker.len())
                                .rev()
                                .find(|&n| c.tail.ends_with(&marker.as_bytes()[..n]))
                                .unwrap_or(0);
                            let count = c.tail.len() - retained;
                            if count > 0 {
                                let bytes = Zeroizing::new(c.tail[..count].to_vec());
                                zeroize::Zeroize::zeroize(&mut c.tail[..count]);
                                c.tail.drain(..count);
                                return Some(bytes);
                            }
                        }
                        ChannelMsg::Failure | ChannelMsg::Close => return None,
                        _ => {}
                    }
                }
            }
        }
    }
    pub(super) async fn restore(&mut self) -> anyhow::Result<()> {
        if let Self::Terminal(channel) = self {
            return channel.restore().await;
        }
        // Wait for the bootstrap trap before showing a startup error or releasing its consumer.
        let recovery = tokio::time::timeout(Duration::from_secs(5), async {
            let cancel = format!("\n#oxideterm-cancel-{}\n", self.token());
            self.write(cancel.as_bytes()).await?;
            while self.recv().await.is_some() {}
            Ok::<_, anyhow::Error>(())
        })
        .await;
        if let Self::Dedicated(c) = self {
            if let Some(channel) = c.channel.as_ref() {
                let _ = tokio::time::timeout(Duration::from_secs(5), async {
                    if !matches!(recovery, Ok(Ok(()))) {
                        let _ = channel.signal(russh::Sig::TERM).await;
                    }
                    channel.close().await
                })
                .await;
            }
            c.channel.take();
            c.restored = true;
        }
        Ok(())
    }

    pub(super) async fn open(
        fs: &NodeAgentIdeFileSystem,
        node: &NodeId,
    ) -> Result<Self, AgentError> {
        let resolved = fs.acquire_ide_connection(node).await?;
        let selected = resolved.handle.select_command_channel().await
            .map_err(|_| AgentError::StartFailed("SSH channel unavailable; complete interactive login in a terminal and use the current terminal".into()))?;
        let exec = selected == oxideterm_ssh::SshCommandChannel::Exec;
        let mode = match selected {
            oxideterm_ssh::SshCommandChannel::Exec => None,
            oxideterm_ssh::SshCommandChannel::Shell => Some(false),
            oxideterm_ssh::SshCommandChannel::PtyShell => Some(true),
        };
        let attempt = async {
            let channel = resolved
                .handle
                .open_session_channel()
                .await
                .map_err(|_| AgentError::ChannelClosed)?;
            let pty = mode == Some(true);
            let mut stream = Self::Dedicated(DedicatedIdeChannel {
                channel: Some(channel),
                token: uuid::Uuid::new_v4().simple().to_string(),
                pty,
                exec,
                restored: false,
                tail: Zeroizing::new(Vec::new()),
                _owner: fs.scope.clone().ok_or(AgentError::ChannelClosed)?,
            });
            let Self::Dedicated(c) = &mut stream else {
                unreachable!()
            };
            let channel = c.channel.as_ref().ok_or(AgentError::ChannelClosed)?;
            if mode.is_none() {
                channel
                    .exec(true, "/bin/sh")
                    .await
                    .map_err(|_| AgentError::ChannelClosed)?;
            } else {
                if pty {
                    channel
                        .request_pty(
                            true,
                            "xterm",
                            120,
                            40,
                            0,
                            0,
                            &[(russh::Pty::ECHO, 0), (russh::Pty::ICANON, 0)],
                        )
                        .await
                        .map_err(|_| AgentError::ChannelClosed)?;
                }
                channel
                    .request_shell(true)
                    .await
                    .map_err(|_| AgentError::ChannelClosed)?;
            }
            let marker = format!("\x1e{}:CHANNEL_READY\x1f", stream.token());
            let command = format!(
                "printf '\\036%s:CHANNEL_READY\\037\\n' '{}'{}",
                stream.token(),
                if pty { "\r" } else { "\n" }
            );
            stream
                .write(command.as_bytes())
                .await
                .map_err(|_| AgentError::ChannelClosed)?;
            let mut output = Zeroizing::new(Vec::new());
            loop {
                let bytes = stream.recv().await.ok_or(AgentError::ChannelClosed)?;
                output.extend_from_slice(&bytes);
                if memchr::memmem::find(&output, marker.as_bytes()).is_some() {
                    return Ok(stream);
                }
                if output.len() > 64 * 1024 {
                    return Err(AgentError::ChannelClosed);
                }
            }
        };
        match tokio::time::timeout(Duration::from_secs(10), attempt).await {
            Ok(Ok(channel)) => Ok(channel),
            _ => Err(AgentError::StartFailed(if exec {
                "SSH exec initialization failed".into()
            } else {
                "SSH shell initialization failed; complete interactive login in a terminal and use the current terminal".into()
            })),
        }
    }
}

#[cfg(test)]
#[path = "channel_tests.rs"]
mod tests;
