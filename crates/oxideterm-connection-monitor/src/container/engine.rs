// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! CLI families the container panel can drive.
//!
//! `wslc` is Microsoft's WSL container CLI. It follows the Docker command
//! surface but only accepts `--format json|table`, supports no pods, and has no
//! pause/unpause or socket/TCP remoting, so capability flags live here instead
//! of leaking engine checks into the UI.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerEngine {
    Docker,
    Podman,
    Wslc,
}

impl ContainerEngine {
    /// Discovery and rendering order; the first reachable engine is the
    /// default instance so Docker keeps precedence over Podman when both run.
    pub const ALL: [Self; 3] = [Self::Docker, Self::Podman, Self::Wslc];

    pub fn id(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
            Self::Wslc => "wslc",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "docker" => Some(Self::Docker),
            "podman" => Some(Self::Podman),
            "wslc" => Some(Self::Wslc),
            _ => None,
        }
    }

    pub fn program(self) -> &'static str {
        self.id()
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Docker => "sidebar.host_container.engines.docker",
            Self::Podman => "sidebar.host_container.engines.podman",
            Self::Wslc => "sidebar.host_container.engines.wslc",
        }
    }

    /// Pods are a Podman concept; Docker and `wslc` have no pod inventory.
    pub fn supports_pods(self) -> bool {
        self == Self::Podman
    }

    pub fn supports_pause(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    /// Socket and TCP remoting both go through the engine CLI, so only engines
    /// whose CLI exposes `-H`/`--url` offer them.
    pub fn supports_remoting(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    pub fn supports_image_history(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    /// Container rename exists in Docker/Podman; `wslc` has no rename verb.
    pub fn supports_container_maintenance(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    /// Only Podman exposes `healthcheck run`; Docker has no such subcommand.
    pub fn supports_healthcheck_action(self) -> bool {
        self == Self::Podman
    }

    /// `top` and `diff` read process/filesystem state inside a container.
    pub fn supports_container_inspection(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    /// Manifest inspection is missing from the WSL container CLI.
    pub fn supports_manifest_inspect(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    /// Docker and Podman keep authenticated registries in an `auths` map;
    /// `wslc` has no documented auth file.
    pub fn supports_registry_auth(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    /// `system df` is missing from the WSL container CLI's documented surface.
    pub fn supports_system_df(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    /// `docker stats`/`podman stats` stream unless told otherwise; `wslc stats`
    /// is already a one-shot snapshot and rejects `--no-stream`.
    pub fn supports_stats_no_stream(self) -> bool {
        !matches!(self, Self::Wslc)
    }

    /// `wslc` only removes containers with `remove`; the Docker family uses
    /// `rm`. Image removal is `rmi` everywhere.
    pub fn container_remove_command(self) -> &'static str {
        match self {
            Self::Wslc => "remove",
            _ => "rm",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_ids_round_trip_and_capabilities_are_engine_specific() {
        for engine in ContainerEngine::ALL {
            assert_eq!(ContainerEngine::parse(engine.id()), Some(engine));
        }
        assert_eq!(
            ContainerEngine::parse(" PODMAN "),
            Some(ContainerEngine::Podman)
        );
        assert_eq!(ContainerEngine::parse("containerd"), None);
        assert!(ContainerEngine::Podman.supports_pods());
        assert!(!ContainerEngine::Docker.supports_pods());
        assert!(!ContainerEngine::Wslc.supports_pause());
        assert!(!ContainerEngine::Wslc.supports_remoting());
        assert!(ContainerEngine::Docker.supports_remoting());
    }
}
