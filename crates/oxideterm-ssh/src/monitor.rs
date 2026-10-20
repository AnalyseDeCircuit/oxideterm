// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{sync::Arc, time::Duration};

use oxideterm_connection_monitor::{
    RESOURCE_CHANNEL_OPEN_TIMEOUT, ResourceSampleShell, ResourceSampler, ResourceSamplerFuture,
};

use crate::{
    ConnectionConsumer, DedicatedConnectionLease, ManagedKeyResolver, NodeId, NodeRouter,
    SshConnectionHandle, SshPromptHandler, SshShellChannel,
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
            match connection
                .open_sampling_shell_channel(init_command, false, timeout)
                .await
            {
                Ok(shell) => Ok(Box::new(SshResourceSampleShell {
                    shell,
                    fallback: None,
                    init_command: init_command.to_owned(),
                    pty_attempted: false,
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
    ) -> ResourceSamplerFuture<'a, Result<String, String>> {
        Box::pin(async move {
            match self
                .shell
                .sample_until(command, end_marker, timeout, max_output_size)
                .await
            {
                Ok(output) => Ok(output),
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
                            if let Ok(output) = self
                                .shell
                                .sample_until(command, end_marker, timeout, max_output_size)
                                .await
                            {
                                return Ok(output);
                            }
                        }
                    }
                    match self.fallback.as_ref() {
                        Some(fallback) => fallback
                            .run_command(command, timeout, max_output_size)
                            .await
                            .map_err(|error| error.to_string()),
                        None => Err(error.to_string()),
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
        _end_marker: &'a str,
        timeout: Duration,
        max_output_size: usize,
    ) -> ResourceSamplerFuture<'a, Result<String, String>> {
        Box::pin(async move {
            self.connection
                .run_command(command, timeout, max_output_size)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}
