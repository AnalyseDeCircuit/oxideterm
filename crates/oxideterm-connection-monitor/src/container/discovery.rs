// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Engine detection.
//!
//! One probe script reports every reachable engine with the most direct
//! transport that answers. Docker and Podman are probed socket → TCP → CLI, so
//! a host where Podman listens on its socket *and* has a working CLI yields a
//! single socket-backed Podman instance. `wslc` has no remoting and is always a
//! CLI instance.

use crate::container::engine::ContainerEngine;
use crate::container::instance::{ContainerInstance, dedupe_instances};
use crate::container::transport::{ContainerTransport, ContainerTransportKind};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContainerDiscovery {
    pub instances: Vec<ContainerInstance>,
    /// Engines whose CLI exists but that could not be reached, plus engines
    /// whose CLI is missing. Only reported when no instance was found.
    pub unavailable: Vec<ContainerUnavailable>,
    /// Set when discovery could not run at all, which is distinct from a host
    /// where every engine is simply absent or stopped.
    pub error: Option<ContainerDiscoveryError>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContainerDiscoveryError {
    /// The POSIX probe never ran because the Windows host has no Bash to run
    /// it with; `message` is the helper's own explanation.
    MissingShell { message: String },
}

impl ContainerDiscoveryError {
    pub fn message(&self) -> &str {
        match self {
            Self::MissingShell { message } => message,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerUnavailable {
    pub engine: ContainerEngine,
    pub detail: Option<String>,
}

const SECTION_START: &str = "===CONTAINER_DISCOVER===";
const SECTION_END: &str = "===CONTAINER_DISCOVER_END===";
const ROW_INSTANCE: &str = "INST";
const ROW_UNAVAILABLE: &str = "UNAVAILABLE";

/// Version probe expression per engine. Docker and Podman print only the
/// daemon version; `wslc` reports its session manager version as JSON.
pub fn discovery_command(os_type: &str) -> String {
    match os_type {
        "Windows" | "windows" => crate::container::windows::posix_via_bash(
            &posix_discovery(),
            "CONTAINER_DISCOVER",
            "Git Bash (bash.exe) not found",
        ),
        _ => posix_discovery(),
    }
}

/// Deadline for one engine probe, in whole seconds.
const PROBE_TIMEOUT_SECONDS: u32 = 4;

fn posix_discovery() -> String {
    posix_discovery_with_deadline(PROBE_TIMEOUT_SECONDS)
}

fn posix_discovery_with_deadline(deadline_seconds: u32) -> String {
    let mut script = String::new();
    script.push_str(&format!("echo '{SECTION_START}'; "));
    // Every probe goes through one bounded runner. GNU `timeout` is preferred,
    // but Git Bash/MSYS resolve `timeout` to Windows' own tool, which waits for
    // a key instead of running a command, so the fallback starts the probe in
    // the background and kills it at the deadline. A probe that cannot be
    // bounded fails (and the engine is reported unavailable) instead of
    // hanging the panel on a daemon that accepts but never answers.
    script.push_str("oxide_deadline=");
    script.push_str(&deadline_seconds.to_string());
    script.push_str(
        "; if timeout --version >/dev/null 2>&1; then \
         oxide_probe() { timeout \"$oxide_deadline\" \"$@\"; }; \
         else oxide_probe() { \"$@\" & oxide_child=$!; \
         ( sleep \"$oxide_deadline\"; kill -9 \"$oxide_child\" 2>/dev/null ) >/dev/null 2>&1 & oxide_watch=$!; \
         wait \"$oxide_child\"; oxide_rc=$?; \
         kill \"$oxide_watch\" 2>/dev/null; return $oxide_rc; }; \
         fi; ",
    );

    // Docker: unix socket → DOCKER_HOST → desktop socket → rootless socket → TCP.
    // The TLS column records what the probe actually did: the loop above probes
    // plaintext, so a daemon that answers there must not gain `--tlsverify`
    // just because it happens to listen on 2376.
    script.push_str(
        "if command -v docker >/dev/null 2>&1; then \
         oxide_ep=''; oxide_kind=''; oxide_v=''; oxide_tls=0; \
         for oxide_candidate in \"unix:///var/run/docker.sock\" \"${DOCKER_HOST}\" \"unix://${HOME}/.docker/run/docker.sock\" \"unix:///run/user/$(id -u)/docker.sock\" \"tcp://127.0.0.1:2375\"; do \
         case \"$oxide_candidate\" in unix://*|npipe://*) oxide_kind=SOCKET ;; tcp://*) oxide_kind=TCP ;; *) continue ;; esac; \
         oxide_v=$(oxide_probe docker -H \"$oxide_candidate\" version --format '{{.Server.Version}}' 2>/dev/null); oxide_rc=$?; \
         if [ \"$oxide_rc\" -eq 0 ] && [ -n \"$oxide_v\" ]; then oxide_ep=\"$oxide_candidate\"; break; fi; \
         done; \
         if [ -z \"$oxide_ep\" ]; then \
         oxide_v=$(oxide_probe docker -H 'tcp://127.0.0.1:2376' --tlsverify version --format '{{.Server.Version}}' 2>/dev/null); oxide_rc=$?; \
         if [ \"$oxide_rc\" -eq 0 ] && [ -n \"$oxide_v\" ]; then oxide_ep='tcp://127.0.0.1:2376'; oxide_kind=TCP; oxide_tls=1; fi; \
         fi; \
         if [ -n \"$oxide_ep\" ]; then \
         printf 'INST\\tDOCKER\\t%s\\t%s\\t%s\\t%s\\n' \"$oxide_kind\" \"$oxide_ep\" \"$oxide_v\" \"$oxide_tls\"; \
         else \
         oxide_v=$(oxide_probe docker version --format '{{.Server.Version}}' 2>/dev/null); oxide_rc=$?; \
         if [ \"$oxide_rc\" -eq 0 ] && [ -n \"$oxide_v\" ]; then \
         printf 'INST\\tDOCKER\\tCLI\\t\\t%s\\t0\\n' \"$oxide_v\"; \
         else \
         oxide_detail=$(oxide_probe docker version 2>&1 >/dev/null | head -n 1 | tr '\\t' ' '); \
         printf 'UNAVAILABLE\\tDOCKER\\t%s\\n' \"${oxide_detail:-docker version did not answer}\"; \
         fi; \
         fi; \
         else printf 'UNAVAILABLE\\tDOCKER\\t%s\\n' 'docker CLI not found'; fi; ",
    );

    // Podman: rootful socket → user socket → XDG socket → TCP → CLI. None of
    // these probes negotiates TLS, so every row carries `0`.
    script.push_str(
        "if command -v podman >/dev/null 2>&1; then \
         oxide_ep=''; oxide_kind=''; oxide_v=''; \
         for oxide_candidate in \"unix:///run/podman/podman.sock\" \"unix:///run/user/$(id -u)/podman/podman.sock\" \"unix://${XDG_RUNTIME_DIR}/podman/podman.sock\" \"tcp://127.0.0.1:2375\"; do \
         case \"$oxide_candidate\" in unix://*) oxide_kind=SOCKET ;; tcp://*) oxide_kind=TCP ;; *) continue ;; esac; \
         oxide_v=$(oxide_probe podman --url \"$oxide_candidate\" info --format '{{.Version.Version}}' 2>/dev/null); \
         // PODMAN_ON_WINDOWS: podman.exe exits 125 even when it answers, and a
         // failed machine connection prints its warning on stdout, so both the
         // exit code and the output shape lie: accept only digit-led versions.
         if [ -n \"$oxide_v\" ] && [ -z \"${oxide_v%%[0-9]*}\" ]; then oxide_ep=\"$oxide_candidate\"; break; fi; \
         done; \
         if [ -n \"$oxide_ep\" ]; then \
         printf 'INST\\tPODMAN\\t%s\\t%s\\t%s\\t0\\n' \"$oxide_kind\" \"$oxide_ep\" \"$oxide_v\"; \
         else \
         oxide_v=$(oxide_probe podman version --format '{{.Server.Version}}' 2>/dev/null); \
         if [ -n \"$oxide_v\" ] && [ -z \"${oxide_v%%[0-9]*}\" ]; then \
         printf 'INST\\tPODMAN\\tCLI\\t\\t%s\\t0\\n' \"$oxide_v\"; \
         else \
         oxide_detail=$(oxide_probe podman version 2>&1 >/dev/null | head -n 1 | tr '\\t' ' '); \
         printf 'UNAVAILABLE\\tPODMAN\\t%s\\n' \"${oxide_detail:-podman version did not answer}\"; \
         fi; \
         fi; \
         else printf 'UNAVAILABLE\\tPODMAN\\t%s\\n' 'podman CLI not found'; fi; ",
    );

    // `wslc` is a snapshot CLI with no remoting and no `--format json` for
    // `info`, so both probes share the same bounded runner as the others.
    script.push_str(
        "if command -v wslc >/dev/null 2>&1; then \
         oxide_v=$(oxide_probe wslc version --format json 2>/dev/null | sed -n 's/.*\"Version\":\"\\([^\"]*\\)\".*/\\1/p' | head -n 1); \
         if [ -n \"$oxide_v\" ]; then printf 'INST\\tWSLC\\tCLI\\t\\t%s\\t0\\n' \"$oxide_v\"; \
         else \
         oxide_detail=$(oxide_probe wslc info 2>&1 >/dev/null | head -n 1 | tr '\\t' ' '); \
         printf 'UNAVAILABLE\\tWSLC\\t%s\\n' \"${oxide_detail:-wslc did not answer}\"; fi; \
         else printf 'UNAVAILABLE\\tWSLC\\t%s\\n' 'wslc CLI not found'; fi; ",
    );

    script.push_str(&format!("echo '{SECTION_END}'"));
    script
}

pub fn parse_discovery(output: &str) -> ContainerDiscovery {
    let Some(section) = extract_section(output, "CONTAINER_DISCOVER") else {
        return ContainerDiscovery::default();
    };
    let mut instances = Vec::new();
    let mut unavailable = Vec::new();
    let mut error = None;

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let mut columns = line.split('\t');
        match columns.next() {
            Some(ROW_INSTANCE) => {
                let Some(engine) = columns.next().and_then(ContainerEngine::parse) else {
                    continue;
                };
                let Some(kind) = columns.next().and_then(ContainerTransportKind::parse) else {
                    continue;
                };
                let endpoint = columns.next().unwrap_or("").trim();
                let version = columns.next().unwrap_or("");
                // The probe reports its own TLS mode: a plaintext probe that
                // answered on 2376 must stay plaintext, and `https://` keys are
                // TLS by scheme even when no flag column arrived.
                let tls = columns
                    .next()
                    .map(str::trim)
                    .is_some_and(|flag| flag == "1" || flag.eq_ignore_ascii_case("true"))
                    || endpoint.starts_with("https://");
                if kind != ContainerTransportKind::Cli && !engine.supports_remoting() {
                    continue;
                }
                let Some(transport) = ContainerTransport::parse(kind.id(), endpoint, tls) else {
                    continue;
                };
                instances.push(ContainerInstance::new(engine, transport, Some(version)));
            }
            Some(ROW_UNAVAILABLE) => {
                // The Windows bridge prints `UNAVAILABLE<TAB><reason>` in place
                // of the whole probe when no Bash exists, so the engine column
                // is missing. That is a discovery failure, not an absent
                // engine, and the panel must be able to say so.
                let label = columns.next().unwrap_or("");
                let Some(engine) = ContainerEngine::parse(label) else {
                    let message = label.trim();
                    if error.is_none() && !message.is_empty() {
                        error = Some(ContainerDiscoveryError::MissingShell {
                            message: message.chars().take(200).collect(),
                        });
                    }
                    continue;
                };
                let detail = columns
                    .next()
                    .map(str::trim)
                    .filter(|detail| !detail.is_empty())
                    .map(|detail| detail.chars().take(200).collect::<String>());
                unavailable.push(ContainerUnavailable { engine, detail });
            }
            _ => {}
        }
    }

    let instances = dedupe_instances(instances);
    unavailable.retain(|entry| {
        !instances
            .iter()
            .any(|instance| instance.engine == entry.engine)
    });
    unavailable.sort_by_key(|entry| entry.engine);
    ContainerDiscovery {
        instances,
        unavailable,
        error,
    }
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    Some(after_start.split_once(&end)?.0.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::transport::ContainerTransport;

    #[test]
    fn discovery_keeps_one_instance_per_engine_with_socket_precedence() {
        let output = concat!(
            "===CONTAINER_DISCOVER===\n",
            "INST\tPODMAN\tCLI\t\t5.8.2\t0\n",
            "INST\tPODMAN\tSOCKET\tunix:///run/user/1000/podman/podman.sock\t5.8.2\t0\n",
            "INST\tDOCKER\tTCP\ttcp://127.0.0.1:2376\t28.0.1\t1\n",
            "UNAVAILABLE\tWSLC\twslc CLI not found\n",
            "===CONTAINER_DISCOVER_END===",
        );
        let discovery = parse_discovery(output);
        assert_eq!(discovery.instances.len(), 2);
        assert_eq!(discovery.instances[0].engine, ContainerEngine::Docker);
        assert_eq!(
            discovery.instances[0].transport,
            ContainerTransport::Tcp {
                endpoint: "tcp://127.0.0.1:2376".into(),
                tls: true
            }
        );
        assert_eq!(discovery.instances[1].engine, ContainerEngine::Podman);
        assert_eq!(
            discovery.instances[1].transport,
            ContainerTransport::Socket {
                endpoint: "unix:///run/user/1000/podman/podman.sock".into()
            }
        );
        assert_eq!(discovery.unavailable.len(), 1);
        assert_eq!(discovery.unavailable[0].engine, ContainerEngine::Wslc);
        assert_eq!(
            discovery.unavailable[0].detail.as_deref(),
            Some("wslc CLI not found")
        );
        assert_eq!(discovery.error, None);
    }

    #[test]
    fn plaintext_probe_on_2376_stays_plaintext() {
        // Reaching a daemon on 2376 without `--tlsverify` proves the endpoint
        // serves plaintext; marking it TLS would add a flag every later command
        // fails with.
        let output = concat!(
            "===CONTAINER_DISCOVER===\n",
            "INST\tDOCKER\tTCP\ttcp://127.0.0.1:2376\t28.0.1\t0\n",
            "===CONTAINER_DISCOVER_END===",
        );
        let discovery = parse_discovery(output);
        assert_eq!(
            discovery.instances[0].transport,
            ContainerTransport::Tcp {
                endpoint: "tcp://127.0.0.1:2376".into(),
                tls: false
            }
        );
    }

    #[test]
    fn missing_bash_becomes_a_discovery_error_not_an_absent_engine() {
        let output = concat!(
            "===CONTAINER_DISCOVER===\n",
            "UNAVAILABLE\tGit Bash (bash.exe) not found\n",
            "===CONTAINER_DISCOVER_END===",
        );
        let discovery = parse_discovery(output);
        assert!(discovery.instances.is_empty());
        assert!(discovery.unavailable.is_empty());
        assert_eq!(
            discovery.error,
            Some(ContainerDiscoveryError::MissingShell {
                message: "Git Bash (bash.exe) not found".to_string()
            })
        );
        assert_eq!(
            discovery
                .error
                .as_ref()
                .map(ContainerDiscoveryError::message),
            Some("Git Bash (bash.exe) not found")
        );
    }

    #[test]
    fn discovery_rejects_remoting_for_engines_without_it() {
        let output = concat!(
            "===CONTAINER_DISCOVER===\n",
            "INST\tWSLC\tSOCKET\tunix:///run/user/1000/podman/podman.sock\t3.0.3\n",
            "INST\tWSLC\tCLI\t\t3.0.3\n",
            "INST\tDOCKER\tSOCKET\ttcp://127.0.0.1:2375\t28.0.1\n",
            "===CONTAINER_DISCOVER_END===",
        );
        let discovery = parse_discovery(output);
        assert_eq!(discovery.instances.len(), 1);
        assert_eq!(discovery.instances[0].engine, ContainerEngine::Wslc);
        assert_eq!(discovery.instances[0].transport, ContainerTransport::Cli);
    }

    #[test]
    fn discovery_without_section_is_empty() {
        let discovery = parse_discovery("docker: command not found\n");
        assert!(discovery.instances.is_empty());
        assert!(discovery.unavailable.is_empty());
        assert_eq!(discovery.error, None);
    }

    #[test]
    fn discovery_commands_cover_every_transport_per_engine() {
        let posix = discovery_command("Linux");
        assert!(posix.contains("unix:///var/run/docker.sock"));
        assert!(posix.contains("tcp://127.0.0.1:2375"));
        assert!(posix.contains("tcp://127.0.0.1:2376"));
        assert!(posix.contains("podman --url"));
        assert!(posix.contains("wslc version --format json"));
        assert!(posix.starts_with("echo '===CONTAINER_DISCOVER==='; "));
        assert!(posix.ends_with("echo '===CONTAINER_DISCOVER_END==='"));
        assert!(posix.contains("timeout --version"));
        // Only the dedicated TLS probe may ask for `--tlsverify`.
        assert_eq!(posix.matches("--tlsverify").count(), 1);

        // Windows hosts run the same POSIX probe through Git Bash, wrapped so
        // cmd.exe can start it.
        let windows = discovery_command("Windows");
        assert!(windows.contains("FromBase64String("));
        assert!(windows.contains("===CONTAINER_DISCOVER==="));
        assert!(windows.contains("Git Bash"));
    }

    #[test]
    fn every_probe_runs_through_the_bounded_runner() {
        let script = posix_discovery();
        assert!(script.contains("oxide_deadline=4"));
        // No probe may bypass the runner, `wslc` included.
        for probe in [
            "oxide_probe docker -H",
            "oxide_probe podman --url",
            "oxide_probe wslc version",
            "oxide_probe wslc info",
            "oxide_probe docker version",
        ] {
            assert!(script.contains(probe), "missing {probe}");
        }
        assert!(!script.contains("$oxide_to"));
        // The fallback keeps a Git Bash host (no GNU timeout) bounded too.
        assert!(script.contains("kill -9"));
        assert!(script.contains("sleep \"$oxide_deadline\""));
    }

    #[test]
    fn probe_deadline_is_configurable_for_tests() {
        assert!(posix_discovery_with_deadline(9).contains("oxide_deadline=9"));
    }
}
