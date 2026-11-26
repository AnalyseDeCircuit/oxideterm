// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Snapshot command generation.
//!
//! One remote exec fills the whole inventory: containers first (the fast,
//! frequently changing part) followed by images, volumes, networks and — on
//! Podman — pods. Each section is capped so a host with thousands of images
//! cannot flood the delivery channel. Output never leaves the host's shell: it
//! is parsed by [`crate::container::parse_snapshot`] on the app side.

use crate::container::engine::ContainerEngine;
use crate::container::instance::ContainerInstance;

pub fn snapshot_command(os_type: &str, instance: &ContainerInstance) -> String {
    match os_type {
        "Windows" | "windows" => crate::container::windows::posix_via_bash(
            &posix_snapshot(instance),
            "CONTAINER",
            "Git Bash (bash.exe) not found",
        ),
        _ => crate::shell::posix_shell_command(&posix_snapshot(instance)),
    }
}

/// Records stay one-per-line on every engine: Docker/`wslc` use `--format
/// json`, Podman uses a compact Go template. Arrays are also accepted by the
/// reader, but they are never the projection here because their pretty-printed
/// lines cannot be capped without cutting a record.
fn posix_snapshot(instance: &ContainerInstance) -> String {
    let program = instance.engine.program();
    let prefix = instance.cli_prefix();
    let pods = instance.engine.supports_pods();
    let mut script = String::new();
    script.push_str("echo '===CONTAINER==='; ");
    // Podman's template form prints one compact object per line (status, ports
    // and age projected alongside), so line caps keep whole records. Docker and
    // `wslc` do the same through `--format json`.
    script.push_str(&format!(
        "if command -v {program} >/dev/null 2>&1; then \
         oxide_ctr=$({prefix} ps -a --no-trunc {} 2>/dev/null); oxide_ctr_rc=$?; \
         if [ \"$oxide_ctr_rc\" -eq 0 ]; then \
         printf '%s\\n' \"$oxide_ctr\" | head -n 500 | awk '{{print \"CTR\\t\" $0}}'; ",
        row_format(instance),
    ));
    if !instance.engine.supports_pods() {
        // Docker's `ps` context has no host PID; one batched `inspect` maps the
        // listed ids. It is best effort: a CLI without `inspect --format`
        // leaves the rows without a PID instead of failing the whole snapshot.
        script.push_str(&pid_section(&prefix));
    }
    // Every list reports its own exit code, so a failed inventory is surfaced
    // as an error instead of being piped through `awk` as an empty one.
    script.push_str(&list_section(
        &prefix,
        "images",
        list_projection(instance),
        300,
        "IMG",
        "images",
    ));
    script.push_str(&list_section(
        &prefix,
        "volume ls",
        list_projection(instance),
        300,
        "VOL",
        "volumes",
    ));
    script.push_str(&list_section(
        &prefix,
        "network ls",
        list_projection(instance),
        300,
        "NET",
        "networks",
    ));
    if pods {
        script.push_str(&list_section(
            &prefix,
            "pod ps",
            pod_format(),
            200,
            "POD",
            "pods",
        ));
    }
    script.push_str(
        "else printf 'ERR\\t%s\\n' \"$(printf '%s' \"$oxide_ctr\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
    );
    script.push_str(&format!(
        "else printf 'UNAVAILABLE\\t%s\\n' '{program} CLI not found'; fi; "
    ));
    script.push_str("echo '===CONTAINER_END==='");
    script
}

/// One inventory list. The engine output is captured first so its exit code
/// decides between capped rows and an explicit section error; piping the
/// command straight into `awk` would report a failed listing as empty.
fn list_section(
    prefix: &str,
    command: &str,
    projection: &str,
    limit: usize,
    row: &str,
    name: &str,
) -> String {
    let variable = row.to_ascii_lowercase();
    format!(
        "oxide_{variable}=$({prefix} {command} {projection} 2>/dev/null); oxide_{variable}_rc=$?; \
         if [ \"$oxide_{variable}_rc\" -eq 0 ]; then \
         if [ -n \"$oxide_{variable}\" ]; then \
         printf '%s\\n' \"$oxide_{variable}\" | head -n {limit} | awk '{{print \"{row}\\t\" $0}}'; fi; \
         else printf 'SECERR\\t{name}\\t%s\\n' \"$oxide_{variable}_rc\"; fi; ",
    )
}

/// Podman projects empty `Status`/`Ports` through `{{json .}}`, so the extra
/// columns supply them. Docker keeps the explicit template because the `json`
/// format keyword only exists in CLI 23.0+, while `{{json .}}` works back to
/// 19.03; `wslc` is the opposite and rejects a template.
fn row_format(instance: &ContainerInstance) -> &'static str {
    match instance.engine {
        ContainerEngine::Podman => {
            "--format '{{json .}}\\t{{.Status}}\\t{{.Ports}}\\t{{.RunningFor}}\\t{{.Pid}}'"
        }
        ContainerEngine::Docker => "--format '{{json .}}'",
        ContainerEngine::Wslc => "--format json",
    }
}

/// Images, volumes and networks share one projection: `{{json .}}` reaches
/// back to Docker 19.03, while `wslc` is the opposite and rejects a template.
fn list_projection(instance: &ContainerInstance) -> &'static str {
    match instance.engine {
        ContainerEngine::Podman | ContainerEngine::Docker => "--format '{{json .}}'",
        ContainerEngine::Wslc => "--format json",
    }
}

fn pod_format() -> &'static str {
    "--format '{{json .}}\\t{{.Created}}'"
}

/// Maps every listed container to its host-side init PID. Podman exposes the
/// PID in the `ps` template; Docker only reports it through `inspect`, so the
/// ids are inspected in one call and `0` (stopped containers) is dropped.
fn pid_section(prefix: &str) -> String {
    format!(
        "oxide_pids=$({prefix} inspect --format '{{{{.Id}}}} {{{{.State.Pid}}}}' $({prefix} ps -aq 2>/dev/null | head -n 500) 2>/dev/null); \
         printf '%s\\n' \"$oxide_pids\" | head -n 500 | awk 'NF >= 2 && $2 != 0 {{print \"PID\\t\" $1 \"\\t\" $2}}'; "
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::engine::ContainerEngine;
    use crate::container::transport::ContainerTransport;

    fn instance(engine: ContainerEngine, transport: ContainerTransport) -> ContainerInstance {
        ContainerInstance::new(engine, transport, Some("1.0.0"))
    }

    #[test]
    fn docker_snapshots_carry_a_batched_host_pid_map() {
        let script = crate::shell::decoded_posix_script(&snapshot_command(
            "Linux",
            &instance(ContainerEngine::Docker, ContainerTransport::Cli),
        ));
        assert!(script.contains("inspect --format '{{.Id}} {{.State.Pid}}'"));
        assert!(script.contains("PID\\t"));
        // Podman reads the pid from the ps template instead of inspecting.
        let podman = crate::shell::decoded_posix_script(&snapshot_command(
            "Linux",
            &instance(
                ContainerEngine::Podman,
                ContainerTransport::Socket {
                    endpoint: "unix:///run/user/1000/podman/podman.sock".into(),
                },
            ),
        ));
        assert!(podman.contains("\\t{{.Pid}}'"));
        assert!(!podman.contains("inspect --format"));
    }

    #[test]
    fn failed_inventory_lists_report_their_exit_code() {
        let script = snapshot_command(
            "Linux",
            &instance(ContainerEngine::Docker, ContainerTransport::Cli),
        );
        for name in ["images", "volumes", "networks"] {
            assert!(
                script.contains(&format!("SECERR\\t{name}\\t%s")),
                "missing marker for {name}"
            );
        }
        // Docker has no pods, so no pod marker is generated for it.
        assert!(!script.contains("SECERR\\tpods"));
        // The exit code comes from the engine, not from `awk` at the end of a
        // pipe, which always succeeds.
        assert!(script.contains("oxide_img_rc=$?"));
        assert!(script.contains("oxide_vol_rc=$?"));
        // The single round trip is preserved: one section pair only.
        assert_eq!(script.matches("===CONTAINER===").count(), 1);
        assert_eq!(script.matches("===CONTAINER_END===").count(), 1);
    }

    #[test]
    fn podman_snapshot_reports_failed_pods_too() {
        let script = snapshot_command(
            "Linux",
            &instance(ContainerEngine::Podman, ContainerTransport::Cli),
        );
        assert!(script.contains("SECERR\\tpods\\t%s"));
        assert!(script.contains("oxide_pod_rc=$?"));
    }

    #[test]
    fn docker_snapshot_uses_the_json_projection_and_socket_endpoint() {
        let script = crate::shell::decoded_posix_script(&snapshot_command(
            "Linux",
            &instance(
                ContainerEngine::Docker,
                ContainerTransport::Socket {
                    endpoint: "unix:///var/run/docker.sock".into(),
                },
            ),
        ));
        // Docker keeps the explicit template: the `json` keyword needs CLI 23.0.
        assert!(script.contains(
            "docker -H 'unix:///var/run/docker.sock' ps -a --no-trunc --format '{{json .}}'"
        ));
        assert!(script.contains("images --format '{{json .}}'"));
        assert!(script.contains("volume ls --format '{{json .}}'"));
        assert!(script.contains("network ls --format '{{json .}}'"));
        assert!(!script.contains("pod ps"));
        assert!(script.contains("UNAVAILABLE"));
        assert!(script.contains("===CONTAINER_END==="));
    }

    #[test]
    fn podman_snapshot_uses_compact_rows_and_pods() {
        let script = crate::shell::decoded_posix_script(&snapshot_command(
            "Linux",
            &instance(
                ContainerEngine::Podman,
                ContainerTransport::Socket {
                    endpoint: "unix:///run/user/1000/podman/podman.sock".into(),
                },
            ),
        ));
        assert!(script.contains(
            "podman --url 'unix:///run/user/1000/podman/podman.sock' ps -a --no-trunc --format '{{json .}}\\t{{.Status}}\\t{{.Ports}}\\t{{.RunningFor}}\\t{{.Pid}}'"
        ));
        assert!(script.contains("pod ps --format '{{json .}}\\t{{.Created}}'"));
    }

    #[test]
    fn cli_transport_has_no_connection_flag() {
        let script = snapshot_command(
            "Linux",
            &instance(ContainerEngine::Podman, ContainerTransport::Cli),
        );
        assert!(script.contains("oxide_ctr=$(podman ps -a"));
        assert!(!script.contains("--url"));
    }

    #[test]
    fn windows_snapshot_runs_the_posix_script_through_git_bash() {
        let script = snapshot_command(
            "Windows",
            &instance(
                ContainerEngine::Docker,
                ContainerTransport::Socket {
                    endpoint: "npipe:////./pipe/docker_engine".into(),
                },
            ),
        );
        assert!(script.contains("FromBase64String("));
        assert!(script.contains("& $oxideBash $oxidePath.Replace"));
        assert!(script.contains("Git Bash"));
        // wslc has no socket/TCP remoting but the shared probe still runs.
        let wslc = snapshot_command(
            "Windows",
            &instance(ContainerEngine::Wslc, ContainerTransport::Cli),
        );
        assert!(wslc.contains("FromBase64String("));
    }
}
