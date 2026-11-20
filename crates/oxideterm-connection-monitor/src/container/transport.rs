// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! How the engine CLI reaches its daemon.
//!
//! Preference order is socket → TCP → plain CLI: a local socket is the least
//! ambiguous endpoint, TCP is only meaningful when the daemon was published,
//! and the bare CLI inherits whatever context the host already configured.
//! Endpoint text always originates from the discovery parser, never from raw
//! user input, but it is quoted anyway because it lands in a shell command.

use serde::{Deserialize, Serialize};

use crate::container::engine::ContainerEngine;
use crate::container::quote::quote_argument;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerTransportKind {
    Socket,
    Tcp,
    Cli,
}

impl ContainerTransportKind {
    pub fn id(self) -> &'static str {
        match self {
            Self::Socket => "socket",
            Self::Tcp => "tcp",
            Self::Cli => "cli",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "socket" => Some(Self::Socket),
            "tcp" => Some(Self::Tcp),
            "cli" => Some(Self::Cli),
            _ => None,
        }
    }

    /// Lower is preferred when one engine answers on several endpoints.
    fn rank(self) -> u8 {
        match self {
            Self::Socket => 0,
            Self::Tcp => 1,
            Self::Cli => 2,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Socket => "sidebar.host_container.transports.socket",
            Self::Tcp => "sidebar.host_container.transports.tcp",
            Self::Cli => "sidebar.host_container.transports.cli",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ContainerTransport {
    /// `unix:///…` on POSIX hosts and `npipe:////./pipe/…` on Windows.
    Socket { endpoint: String },
    /// `tcp://host:port`; `tls` adds `--tlsverify` for daemons on 2376.
    Tcp { endpoint: String, tls: bool },
    /// Whatever daemon the CLI resolves on its own (context, machine, env).
    Cli,
}

impl ContainerTransport {
    pub fn kind(&self) -> ContainerTransportKind {
        match self {
            Self::Socket { .. } => ContainerTransportKind::Socket,
            Self::Tcp { .. } => ContainerTransportKind::Tcp,
            Self::Cli => ContainerTransportKind::Cli,
        }
    }

    pub fn endpoint(&self) -> Option<&str> {
        match self {
            Self::Socket { endpoint } => Some(endpoint),
            Self::Tcp { endpoint, .. } => Some(endpoint),
            Self::Cli => None,
        }
    }

    pub fn tls(&self) -> bool {
        matches!(self, Self::Tcp { tls: true, .. })
    }

    /// Stable identity used for selection and dedupe.
    pub fn id(&self) -> String {
        match self {
            Self::Socket { endpoint } => format!("socket:{endpoint}"),
            Self::Tcp { endpoint, .. } => format!("tcp:{endpoint}"),
            Self::Cli => "cli".to_string(),
        }
    }

    pub fn preference(&self) -> u8 {
        self.kind().rank()
    }

    /// Builds a transport from discovery output, rejecting endpoints that do
    /// not carry the scheme the transport implies.
    pub fn parse(kind: &str, endpoint: &str, tls: bool) -> Option<Self> {
        let endpoint = endpoint.trim();
        let kind = ContainerTransportKind::parse(kind)?;
        // Discovery echoes `DOCKER_HOST` and similar environment values, so a
        // remote endpoint is pasted into a shell line later. Only characters
        // that appear in real engine endpoints are accepted, which keeps a
        // hostile value from leaving its quoted argument on either cmd.exe or sh.
        if kind != ContainerTransportKind::Cli && !endpoint_is_plain(endpoint) {
            return None;
        }
        match kind {
            ContainerTransportKind::Socket => {
                if !(endpoint.starts_with("unix://") || endpoint.starts_with("npipe://")) {
                    return None;
                }
                Some(Self::Socket {
                    endpoint: endpoint.to_string(),
                })
            }
            ContainerTransportKind::Tcp => {
                if !endpoint.starts_with("tcp://") {
                    return None;
                }
                Some(Self::Tcp {
                    endpoint: endpoint.to_string(),
                    tls,
                })
            }
            ContainerTransportKind::Cli => Some(Self::Cli),
        }
    }

    /// The CLI arguments that point the engine at this transport.
    ///
    /// Docker and `wslc`-style CLIs take `-H`; Podman takes `--url`. `wslc`
    /// has no remoting at all: discovery never yields such a transport, and a
    /// directly constructed instance must not emit a flag the engine cannot
    /// accept either. `os_type` selects the shell that will run the generated
    /// line, so the endpoint is quoted in that shell's dialect.
    pub fn connection_flag_for(&self, engine: ContainerEngine, os_type: &str) -> Option<String> {
        if !engine.supports_remoting() {
            return None;
        }
        let Some(endpoint) = self.endpoint() else {
            return None;
        };
        let flag = match engine {
            ContainerEngine::Podman => "--url",
            _ => "-H",
        };
        let mut text = format!("{flag} {}", quote_argument(os_type, endpoint));
        if self.tls() {
            text.push_str(" --tlsverify");
        }
        Some(text)
    }
}

/// Endpoints are pasted into shell lines, so only characters seen in real
/// engine endpoints pass. Quotes, `$`, backticks, whitespace and shell
/// metacharacters are dropped rather than escaped.
fn endpoint_is_plain(endpoint: &str) -> bool {
    !endpoint.is_empty()
        && endpoint.len() <= 512
        && endpoint.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'/' | b':' | b'.' | b'-' | b'_' | b'+' | b'@' | b'%' | b'=' | b'\\' | b'~'
                )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_ids_and_flags_are_engine_specific() {
        let socket = ContainerTransport::Socket {
            endpoint: "unix:///var/run/docker.sock".into(),
        };
        assert_eq!(socket.id(), "socket:unix:///var/run/docker.sock");
        assert_eq!(
            socket.connection_flag_for(ContainerEngine::Docker, "Linux"),
            Some("-H 'unix:///var/run/docker.sock'".to_string())
        );
        assert_eq!(
            socket.connection_flag_for(ContainerEngine::Podman, "Linux"),
            Some("--url 'unix:///var/run/docker.sock'".to_string())
        );
        assert_eq!(
            ContainerTransport::Cli.connection_flag_for(ContainerEngine::Docker, "Linux"),
            None
        );
        // `wslc` has no remoting: a directly built instance must not emit `-H`.
        let socket = ContainerTransport::Socket {
            endpoint: "unix:///run/user/1000/wslc.sock".into(),
        };
        assert_eq!(
            socket.connection_flag_for(ContainerEngine::Wslc, "Linux"),
            None
        );

        let tcp = ContainerTransport::Tcp {
            endpoint: "tcp://10.0.0.2:2376".into(),
            tls: true,
        };
        assert_eq!(
            tcp.connection_flag_for(ContainerEngine::Docker, "Linux"),
            Some("-H 'tcp://10.0.0.2:2376' --tlsverify".to_string())
        );
        assert_eq!(tcp.preference(), 1);
        assert_eq!(ContainerTransport::Cli.preference(), 2);
    }

    #[test]
    fn transport_parse_rejects_mismatched_schemes() {
        assert_eq!(
            ContainerTransport::parse("socket", "unix:///run/podman/podman.sock", false),
            Some(ContainerTransport::Socket {
                endpoint: "unix:///run/podman/podman.sock".into()
            })
        );
        assert_eq!(
            ContainerTransport::parse("socket", "npipe:////./pipe/docker_engine", false),
            Some(ContainerTransport::Socket {
                endpoint: "npipe:////./pipe/docker_engine".into()
            })
        );
        assert!(ContainerTransport::parse("socket", "tcp://127.0.0.1:2375", false).is_none());
        assert!(ContainerTransport::parse("tcp", "unix:///var/run/docker.sock", false).is_none());
        assert_eq!(
            ContainerTransport::parse("cli", "", false),
            Some(ContainerTransport::Cli)
        );
    }

    #[test]
    fn hostile_endpoints_never_reach_a_shell() {
        // Discovery echoes DOCKER_HOST; a value that closes the quoted argument
        // must be dropped instead of embedded.
        for hostile in [
            "tcp://host\"; Write-Output PWN; #",
            "unix:///tmp/it's.sock",
            "unix:///tmp/a b.sock",
            "tcp://host$(id)/x",
            "tcp://host`id`/x",
        ] {
            assert_eq!(ContainerTransport::parse("tcp", hostile, false), None);
            assert_eq!(ContainerTransport::parse("socket", hostile, false), None);
        }
        // Real endpoints still round-trip into a single quoted argument, and
        // Windows lines quote in PowerShell rather than sh.
        let transport = ContainerTransport::Socket {
            endpoint: "npipe:////./pipe/docker_engine".into(),
        };
        for os_type in ["Linux", "Windows"] {
            assert_eq!(
                transport.connection_flag_for(ContainerEngine::Docker, os_type),
                Some("-H 'npipe:////./pipe/docker_engine'".to_string())
            );
        }
    }
}
