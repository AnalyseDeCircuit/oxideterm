// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! One reachable engine on one host.
//!
//! An instance is identified by engine (Docker, Podman, `wslc`) and transport.
//! Discovery can report the same engine several times (socket plus CLI); the
//! dedupe step keeps exactly one instance per engine, which is the uniqueness
//! rule the panel relies on: a host that runs Podman through both its socket
//! and its CLI shows a single Podman entry.

use serde::{Deserialize, Serialize};

use crate::container::engine::ContainerEngine;
use crate::container::quote::sanitized_version;
use crate::container::transport::ContainerTransport;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerInstance {
    pub engine: ContainerEngine,
    pub transport: ContainerTransport,
    #[serde(default)]
    pub server_version: Option<String>,
}

impl ContainerInstance {
    pub fn new(
        engine: ContainerEngine,
        transport: ContainerTransport,
        server_version: Option<&str>,
    ) -> Self {
        Self {
            engine,
            transport,
            server_version: server_version.and_then(sanitized_version),
        }
    }

    /// Stable identity: `<engine>:<transport>`. The version is deliberately
    /// excluded so a daemon upgrade does not invalidate the user's selection.
    pub fn id(&self) -> String {
        format!("{}:{}", self.engine.id(), self.transport.id())
    }

    /// CLI text for a POSIX shell (`sh`/`bash`), e.g. `docker -H 'unix:///…'`.
    ///
    /// Snapshot and stats scripts run under the Git Bash bridge on Windows
    /// too, so they always need this dialect rather than PowerShell.
    pub fn cli_prefix(&self) -> String {
        self.cli_prefix_for("Linux")
    }

    /// CLI text for the shell that runs `os_type` (`Linux` → sh, `Windows` →
    /// PowerShell). Terminal and capture lines use this so the transport
    /// endpoint is quoted in the dialect that will parse it.
    pub fn cli_prefix_for(&self, os_type: &str) -> String {
        match self.transport.connection_flag_for(self.engine, os_type) {
            Some(flag) => format!("{} {}", self.engine.program(), flag),
            None => self.engine.program().to_string(),
        }
    }
}

/// Keeps one instance per engine: the one with the most direct transport
/// (socket over TCP over CLI) and the first discovery result otherwise.
pub fn dedupe_instances(
    instances: impl IntoIterator<Item = ContainerInstance>,
) -> Vec<ContainerInstance> {
    let mut best: Vec<ContainerInstance> = Vec::new();
    for instance in instances {
        match best
            .iter_mut()
            .find(|existing| existing.engine == instance.engine)
        {
            Some(existing) => {
                if instance.transport.preference() < existing.transport.preference() {
                    *existing = instance;
                }
            }
            None => best.push(instance),
        }
    }
    best.sort_by_key(|instance| (instance.engine, instance.transport.preference()));
    best
}

/// The instance the panel opens first: the highest-priority engine (Docker,
/// then Podman, then `wslc`) with its preferred transport.
pub fn preferred_instance(instances: &[ContainerInstance]) -> Option<&ContainerInstance> {
    instances.first()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::transport::ContainerTransport;

    fn socket(endpoint: &str) -> ContainerTransport {
        ContainerTransport::Socket {
            endpoint: endpoint.to_string(),
        }
    }

    #[test]
    fn dedupe_keeps_one_instance_per_engine_with_the_best_transport() {
        let instances = dedupe_instances(vec![
            ContainerInstance::new(
                ContainerEngine::Podman,
                ContainerTransport::Cli,
                Some("5.8.2"),
            ),
            ContainerInstance::new(
                ContainerEngine::Docker,
                ContainerTransport::Tcp {
                    endpoint: "tcp://127.0.0.1:2375".into(),
                    tls: false,
                },
                Some("28.0.1"),
            ),
            ContainerInstance::new(
                ContainerEngine::Podman,
                socket("unix:///run/podman/podman.sock"),
                Some("5.8.2"),
            ),
        ]);
        assert_eq!(instances.len(), 2);
        assert_eq!(instances[0].engine, ContainerEngine::Docker);
        assert_eq!(instances[1].engine, ContainerEngine::Podman);
        assert_eq!(
            instances[1].transport,
            socket("unix:///run/podman/podman.sock")
        );
        assert_eq!(
            preferred_instance(&instances).map(|i| i.engine),
            Some(ContainerEngine::Docker)
        );
    }

    #[test]
    fn instance_identity_ignores_the_daemon_version() {
        let older = ContainerInstance::new(
            ContainerEngine::Docker,
            socket("unix:///var/run/docker.sock"),
            Some("27.0.0"),
        );
        let newer = ContainerInstance::new(
            ContainerEngine::Docker,
            socket("unix:///var/run/docker.sock"),
            Some("28.0.1"),
        );
        assert_eq!(older.id(), newer.id());
        assert_eq!(newer.server_version.as_deref(), Some("28.0.1"));
        assert_eq!(
            newer.cli_prefix(),
            "docker -H 'unix:///var/run/docker.sock'"
        );
    }
}
