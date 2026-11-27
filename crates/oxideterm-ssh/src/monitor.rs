// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{sync::Arc, time::Duration};

use oxideterm_connection_monitor::{
    RESOURCE_CHANNEL_OPEN_TIMEOUT, ResourceSampleError, ResourceSampleShell, ResourceSampler,
    ResourceSamplerFuture,
};

use crate::{
    ConnectionConsumer, DedicatedConnectionLease, ManagedKeyResolver, NodeId, NodeRouter,
    SshConnectionHandle, SshPromptHandler, SshShellChannel, SshTransportError,
    connection_registry::is_fish_login_shell,
};

pub struct DedicatedNodeResourceSampler {
    router: NodeRouter,
    node_id: NodeId,
    prompt_handler: Arc<dyn SshPromptHandler>,
    managed_key_resolver: ManagedKeyResolver,
}

impl DedicatedNodeResourceSampler {
    pub fn new(
        router: NodeRouter,
        node_id: NodeId,
        prompt_handler: Arc<dyn SshPromptHandler>,
        managed_key_resolver: ManagedKeyResolver,
    ) -> Self {
        Self {
            router,
            node_id,
            prompt_handler,
            managed_key_resolver,
        }
    }
}

impl ResourceSampler for DedicatedNodeResourceSampler {
    fn open_shell<'a>(
        &'a self,
        init_command: &'a str,
        timeout: Duration,
    ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
        Box::pin(async move {
            let consumer = ConnectionConsumer::Monitor(uuid::Uuid::new_v4().to_string());
            let lease = Arc::new(
                self.router
                    .acquire_dedicated_connection(
                        &self.node_id,
                        consumer,
                        self.prompt_handler.clone(),
                        self.managed_key_resolver.clone(),
                    )
                    .await
                    .map_err(|error| error.to_string())?,
            );
            let connection = lease.handle().clone();
            // Dedicated connections do not run remote_env probing themselves; consult the
            // node's existing shared connection so Fish login shells avoid non-PTY stdin
            // buffering (which stalls until EOF) on single-channel leases.
            let fish_shell = connection.remote_login_shell_is_fish()
                || self
                    .router
                    .node_remote_env(&self.node_id)
                    .and_then(|env| env.shell)
                    .is_some_and(|shell| is_fish_login_shell(&shell));
            let effective_init = if fish_shell {
                fish_sampling_init_command(init_command)
            } else {
                init_command.to_owned()
            };
            match connection
                .open_sampling_shell_channel(&effective_init, fish_shell, timeout)
                .await
            {
                Ok(shell) => Ok(Box::new(SshResourceSampleShell {
                    shell,
                    fallback: None,
                    init_command: effective_init,
                    pty_attempted: fish_shell,
                    _connection_owner: Some(lease),
                }) as Box<dyn ResourceSampleShell>),
                Err(_) => Ok(Box::new(SshExecResourceSampleShell {
                    connection,
                    _connection_owner: Some(lease),
                }) as Box<dyn ResourceSampleShell>),
            }
        })
    }
}

impl ResourceSampler for SshConnectionHandle {
    fn open_shell<'a>(
        &'a self,
        init_command: &'a str,
        timeout: Duration,
    ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
        Box::pin(async move {
            // Non-PTY Fish buffers stdin until EOF, which would stall the first sample
            // for an entire timeout before falling back. Use one-shot exec directly.
            if self.remote_login_shell_is_fish() {
                return Ok(Box::new(SshExecResourceSampleShell {
                    connection: self.clone(),
                    _connection_owner: None,
                }) as Box<dyn ResourceSampleShell>);
            }
            match self
                .open_sampling_shell_channel(init_command, false, timeout)
                .await
            {
                Ok(shell) => Ok(Box::new(SshResourceSampleShell {
                    shell,
                    fallback: Some(self.clone()),
                    init_command: init_command.to_owned(),
                    pty_attempted: false,
                    _connection_owner: None,
                }) as Box<dyn ResourceSampleShell>),
                Err(_) => Ok(Box::new(SshExecResourceSampleShell {
                    connection: self.clone(),
                    _connection_owner: None,
                }) as Box<dyn ResourceSampleShell>),
            }
        })
    }
}

struct SshResourceSampleShell {
    shell: SshShellChannel,
    fallback: Option<SshConnectionHandle>,
    init_command: String,
    pty_attempted: bool,
    _connection_owner: Option<Arc<DedicatedConnectionLease>>,
}

impl ResourceSampleShell for SshResourceSampleShell {
    fn sample_until<'a>(
        &'a mut self,
        command: &'a str,
        end_marker: &'a str,
        timeout: Duration,
        max_output_size: usize,
    ) -> ResourceSamplerFuture<'a, Result<String, ResourceSampleError>> {
        Box::pin(async move {
            match self
                .shell
                .sample_until(command, end_marker, timeout, max_output_size)
                .await
            {
                Ok(output) => Ok(output),
                Err(SshTransportError::SampleOutputLimit { channel_reusable }) => {
                    Err(ResourceSampleError::OutputLimit { channel_reusable })
                }
                // Multiplexed servers may fall back to one-shot exec. A
                // single-channel lease fails instead of opening a second channel.
                Err(error) => {
                    // Replace only this sampler's channel on the authenticated transport.
                    // Dedicated single-channel leases must not open another channel.
                    if !self.pty_attempted
                        && let Some(connection) = self.fallback.as_ref()
                    {
                        self.pty_attempted = true;
                        let _ = tokio::time::timeout(timeout, self.shell.close()).await;
                        if let Ok(shell) = connection
                            .open_sampling_shell_channel(
                                &self.init_command,
                                true,
                                RESOURCE_CHANNEL_OPEN_TIMEOUT,
                            )
                            .await
                        {
                            self.shell = shell;
                            match self
                                .shell
                                .sample_until(command, end_marker, timeout, max_output_size)
                                .await
                            {
                                Ok(output) => return Ok(output),
                                Err(SshTransportError::SampleOutputLimit { channel_reusable }) => {
                                    return Err(ResourceSampleError::OutputLimit {
                                        channel_reusable,
                                    });
                                }
                                Err(_) => {}
                            }
                        }
                    }
                    match self.fallback.as_ref() {
                        Some(fallback) => {
                            sample_exec(fallback, command, end_marker, timeout, max_output_size)
                                .await
                        }
                        None => Err(error.to_string().into()),
                    }
                }
            }
        })
    }

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
        Box::pin(async move { self.shell.close().await.map_err(|error| error.to_string()) })
    }
}

struct SshExecResourceSampleShell {
    connection: SshConnectionHandle,
    _connection_owner: Option<Arc<DedicatedConnectionLease>>,
}

impl ResourceSampleShell for SshExecResourceSampleShell {
    fn sample_until<'a>(
        &'a mut self,
        command: &'a str,
        end_marker: &'a str,
        timeout: Duration,
        max_output_size: usize,
    ) -> ResourceSamplerFuture<'a, Result<String, ResourceSampleError>> {
        Box::pin(async move {
            sample_exec(
                &self.connection,
                command,
                end_marker,
                timeout,
                max_output_size,
            )
            .await
        })
    }

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

async fn sample_exec(
    connection: &SshConnectionHandle,
    command: &str,
    end_marker: &str,
    timeout: Duration,
    max_output_size: usize,
) -> Result<String, ResourceSampleError> {
    let output = connection
        .run_command_capture(command, timeout, max_output_size)
        .await
        .map_err(|error| ResourceSampleError::Failed(error.to_string()))?;
    if output.truncated {
        return Err(ResourceSampleError::OutputLimit {
            channel_reusable: true,
        });
    }
    if output.exit_code.is_some_and(|code| code != 0) {
        return Err(ResourceSampleError::Failed("Sample command failed".into()));
    }
    if !output
        .stdout
        .lines()
        .any(|line| line.trim_end_matches('\r') == end_marker)
    {
        return Err(ResourceSampleError::Failed(
            "Sample ended without its completion marker".into(),
        ));
    }
    Ok(output.stdout)
}

fn fish_sampling_init_command(init_command: &str) -> String {
    // Silence echo and replace Fish with /bin/sh on the PTY before running the
    // POSIX sampling init sequence so Fish prompts and syntax do not interfere.
    if init_command.is_empty() {
        "stty -echo 2>/dev/null; exec /bin/sh\n".to_string()
    } else {
        format!("stty -echo 2>/dev/null; exec /bin/sh\n{init_command}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fish_shell_detection_and_pty_init_handoff() {
        for fish in [
            "/usr/bin/fish",
            "/opt/homebrew/bin/fish",
            "fish",
            "FISH.EXE ",
        ] {
            assert!(is_fish_login_shell(fish), "{fish}");
        }
        for other in ["/bin/bash", "/bin/zsh", "/usr/bin/sh", "starfish", ""] {
            assert!(!is_fish_login_shell(other), "{other}");
        }
        assert_eq!(
            fish_sampling_init_command("export PS1=''\n"),
            "stty -echo 2>/dev/null; exec /bin/sh\nexport PS1=''\n"
        );
    }
}
