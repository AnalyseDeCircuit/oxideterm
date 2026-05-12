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
            let shell =
                tokio::time::timeout(timeout, self.open_persistent_shell_channel(init_command))
                    .await
                    .map_err(|_| "Timeout opening shell channel".to_string())?
                    .map_err(|error| format!("Failed to open shell channel: {error}"))?;

            Ok(Box::new(SshResourceSampleShell { shell }) as Box<dyn ResourceSampleShell>)
        })
    }
}

struct SshResourceSampleShell {
    shell: SshShellChannel,
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
            self.shell
                .sample_until(command, end_marker, timeout, max_output_size)
                .await
                .map_err(|error| error.to_string())
        })
    }

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
        Box::pin(async move { self.shell.close().await.map_err(|error| error.to_string()) })
    }
}
