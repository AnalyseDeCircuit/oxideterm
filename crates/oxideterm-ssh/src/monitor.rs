// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::time::Duration;

use oxideterm_connection_monitor::{ResourceSampleShell, ResourceSampler, ResourceSamplerFuture};

use crate::{SshConnectionHandle, SshShellChannel};

impl ResourceSampler for SshConnectionHandle {
    fn open_shell<'a>(
        &'a self,
        init_command: &'a str,
        timeout: Duration,
    ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
        Box::pin(async move {
            match tokio::time::timeout(timeout, self.open_persistent_shell_channel(init_command))
                .await
            {
                Ok(Ok(shell)) => Ok(Box::new(SshResourceSampleShell {
                    shell,
                    fallback: self.clone(),
                }) as Box<dyn ResourceSampleShell>),
                Ok(Err(_error)) => Ok(Box::new(SshExecResourceSampleShell {
                    connection: self.clone(),
                }) as Box<dyn ResourceSampleShell>),
                Err(_) => Ok(Box::new(SshExecResourceSampleShell {
                    connection: self.clone(),
                }) as Box<dyn ResourceSampleShell>),
            }
        })
    }
}

struct SshResourceSampleShell {
    shell: SshShellChannel,
    fallback: SshConnectionHandle,
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
                // Some servers accept exec channels but reject or stall
                // interactive shell channels. Fall back to one-shot exec so
                // the health panel can still collect full metrics.
                Err(_) => self
                    .fallback
                    .run_command(command, timeout, max_output_size)
                    .await
                    .map_err(|error| error.to_string()),
            }
        })
    }

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
        Box::pin(async move { self.shell.close().await.map_err(|error| error.to_string()) })
    }
}

struct SshExecResourceSampleShell {
    connection: SshConnectionHandle,
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
