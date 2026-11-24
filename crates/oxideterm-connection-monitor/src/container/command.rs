// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Mutating actions and read-only captures.
//!
//! Commands are composed from the instance's CLI prefix, so every backend and
//! transport shares one code path. Engine differences — `wslc`'s `remove`
//! verb and its missing pause, image-history, manifest and volume-usage
//! support — are handled here by capability checks instead of dialect branches
//! in the panel.

use crate::container::engine::ContainerEngine;
use crate::container::instance::ContainerInstance;
use crate::container::quote::{
    quote_argument, shell_quote_single, validated_container_id, validated_resource_name,
};

/// Structured options for launching one container from an image. The panel
/// collects them in the run dialog; every field is validated before it reaches
/// a command line.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContainerRunOptions {
    /// `--name`; empty omits the flag.
    pub name: String,
    pub detached: bool,
    pub interactive: bool,
    pub auto_remove: bool,
    /// `-p` specs, one per entry (`8080:80`, `127.0.0.1:8080:80/udp`).
    pub ports: Vec<String>,
    /// `-v` specs, one per entry (`/host:/ctr`, `/data:/ctr:ro`).
    pub volumes: Vec<String>,
    /// Program and arguments for the container entrypoint (`/bin/sh -lc ...`).
    /// Shell word-split and re-quoted per token, so operators typed here stay
    /// inside the container command instead of reaching the host shell.
    pub command: String,
}

/// Splits the run dialog's port field into specs. Ports never contain commas,
/// so both a comma- and a line-separated list are accepted.
fn parse_run_specs(field: &str, verb: &str) -> Result<Vec<String>, String> {
    field
        .split([',', '\n', '\r'])
        .map(str::trim)
        .filter(|spec| !spec.is_empty())
        .map(|spec| {
            if spec.chars().any(|character| {
                character.is_control() || matches!(character, '"' | '\'' | '$' | '`' | '\\')
            }) {
                return Err(format!("Invalid {verb} spec."));
            }
            Ok(spec.to_string())
        })
        .collect()
}

/// Splits the volume field into specs. Volumes are one per line; commas inside
/// a line are only treated as separators when every piece carries its own
/// `source:destination` colon, so option lists like `:ro,z` stay intact.
fn parse_volume_specs(field: &str) -> Result<Vec<String>, String> {
    let mut specs = Vec::new();
    for line in field.split('\n') {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let pieces: Vec<&str> = line
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        if pieces.len() > 1 && pieces.iter().all(|piece| piece.contains(':')) {
            specs.extend(pieces.iter().map(|piece| piece.to_string()));
        } else {
            specs.push(line.to_string());
        }
    }
    for spec in &specs {
        if spec.chars().any(|character| {
            character.is_control() || matches!(character, '"' | '\'' | '$' | '`' | '\\')
        }) {
            return Err("Invalid volume spec.".to_string());
        }
    }
    Ok(specs)
}

/// Validates one `-p` spec: `[host:]container[/protocol]` with numeric ports
/// or port ranges. The engine reports anything the CLI rejects, but obvious
/// typos should fail in the dialog instead of opening a dead terminal.
fn validated_port_spec(spec: &str) -> Result<String, String> {
    let address = match spec.split_once('/') {
        Some((address, "tcp" | "udp" | "sctp")) => address,
        Some(_) => return Err("Invalid port spec.".to_string()),
        None => spec,
    };
    let mut ports = address.rsplitn(3, ':');
    validated_port_range(ports.next().unwrap_or_default())?;
    if let Some(host_port) = ports.next() {
        validated_port_range(host_port)?;
    }
    if let Some(host) = ports.next() {
        let valid = match host.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            Some(ip) => ip.parse::<std::net::Ipv6Addr>().is_ok(),
            None => host.parse::<std::net::Ipv4Addr>().is_ok(),
        };
        if !valid {
            return Err("Invalid port spec.".to_string());
        }
    }
    Ok(spec.to_string())
}

fn validated_port_range(part: &str) -> Result<(), String> {
    let (start, end) = part.split_once('-').unwrap_or((part, part));
    match (start.parse::<u16>(), end.parse::<u16>()) {
        (Ok(start), Ok(end)) if start > 0 && start <= end => Ok(()),
        _ => Err("Invalid port spec.".to_string()),
    }
}

/// Validates one `-v` spec: at least `source:destination`, optional trailing
/// option list (`:ro,z`).
fn validated_volume_spec(spec: &str) -> Result<String, String> {
    if !spec.contains(':') {
        return Err("Invalid volume spec.".to_string());
    }
    Ok(spec.to_string())
}

pub(super) fn run_arguments(
    os_type: &str,
    image: &str,
    options: &ContainerRunOptions,
    mut quote: impl FnMut(&str) -> String,
) -> Result<String, String> {
    let image = validated_resource_name(image)?;
    let mut args = String::new();
    if options.detached {
        args.push_str("-d ");
    }
    if options.interactive {
        args.push_str("-it ");
    }
    if options.auto_remove {
        args.push_str("--rm ");
    }
    let name = options.name.trim();
    if !name.is_empty() {
        // `--name --all` would become a `docker` flag, so the name must pass
        // the same identifier rules as every other user-supplied reference.
        let name = validated_resource_name(name)?;
        args.push_str(&format!("--name {} ", quote(name)));
    }
    for port in &options.ports {
        args.push_str(&format!("-p {} ", quote(port)));
    }
    for volume in &options.volumes {
        args.push_str(&format!("-v {} ", quote(volume)));
    }
    args.push_str(&quote(image));
    for word in split_container_command(&options.command, os_type)? {
        args.push(' ');
        args.push_str(&quote(&word));
    }
    Ok(args)
}

/// Splits the run dialog's container command into argv words.
///
/// The words are appended as separate quoted arguments, so a `;` or `&&` the
/// user typed ends up inside the container command instead of terminating the
/// generated host-shell line. Quotes group words the way a shell would.
pub fn split_container_command(command: &str, os_type: &str) -> Result<Vec<String>, String> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(Vec::new());
    }
    if command.chars().any(char::is_control) {
        return Err("Control characters are not allowed.".to_string());
    }
    #[derive(Clone, Copy, PartialEq)]
    enum Quote {
        None,
        Single,
        Double,
    }
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = Quote::None;
    let mut escaped = false;
    let mut pending_word = false;
    for character in command.chars() {
        if escaped {
            word.push(character);
            escaped = false;
            pending_word = true;
            continue;
        }
        match quote {
            Quote::Single => {
                if character == '\'' {
                    quote = Quote::None;
                } else {
                    word.push(character);
                }
            }
            Quote::Double => match character {
                '"' => quote = Quote::None,
                '\\' if !crate::container::quote::is_windows(os_type) => escaped = true,
                _ => word.push(character),
            },
            Quote::None => match character {
                '\'' => {
                    quote = Quote::Single;
                    pending_word = true;
                }
                '"' => {
                    quote = Quote::Double;
                    pending_word = true;
                }
                '\\' if !crate::container::quote::is_windows(os_type) => {
                    escaped = true;
                    pending_word = true;
                }
                c if c.is_whitespace() => {
                    if pending_word {
                        words.push(std::mem::take(&mut word));
                        pending_word = false;
                    }
                }
                c => {
                    word.push(c);
                    pending_word = true;
                }
            },
        }
    }
    if escaped || quote != Quote::None {
        return Err("Unterminated quote in command.".to_string());
    }
    if pending_word {
        words.push(word);
    }
    Ok(words)
}

/// Builds the stopped-container prune command. Docker and Podman share the
/// `container prune -f` surface; `wslc` does not document it.
pub fn container_prune_command(
    os_type: &str,
    instance: &ContainerInstance,
) -> Result<String, String> {
    Ok(wrapped_command(
        os_type,
        format!("{} container prune -f", instance.cli_prefix_for(os_type)),
    ))
}

/// Parses the run dialog's free-text fields into validated specs.
pub fn parse_run_options(ports: &str, volumes: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let ports = parse_run_specs(ports, "port")?
        .into_iter()
        .map(|spec| validated_port_spec(&spec))
        .collect::<Result<Vec<_>, _>>()?;
    let volumes = parse_volume_specs(volumes)?
        .into_iter()
        .map(|spec| validated_volume_spec(&spec))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((ports, volumes))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Kill,
    Pause,
    Unpause,
    Remove,
}

impl ContainerAction {
    pub fn id(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Kill => "kill",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::Remove => "remove",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Start => "sidebar.host_container.actions.start",
            Self::Stop => "sidebar.host_container.actions.stop",
            Self::Restart => "sidebar.host_container.actions.restart",
            Self::Kill => "sidebar.host_container.actions.kill",
            Self::Pause => "sidebar.host_container.actions.pause",
            Self::Unpause => "sidebar.host_container.actions.unpause",
            Self::Remove => "sidebar.host_container.actions.remove",
        }
    }

    pub fn is_destructive(self) -> bool {
        matches!(self, Self::Remove | Self::Kill)
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ContainerActionAvailability {
    pub can_start: bool,
    pub can_stop: bool,
    pub can_restart: bool,
    /// `docker kill` is the escape hatch for a wedged running container.
    pub can_kill: bool,
    pub can_pause: bool,
    pub can_unpause: bool,
    /// Removal is limited to containers that never ran or fully exited, so a
    /// destructive remove never sits behind a running workload.
    pub can_remove: bool,
    /// Live log reads tolerate a paused freezer; exec does not.
    pub can_use_live_tools: bool,
    pub can_exec: bool,
}

/// Unknown states expose no mutating affordances.
pub fn container_action_availability(state: &str) -> ContainerActionAvailability {
    let state = state.trim().to_ascii_lowercase();
    let running = matches!(state.as_str(), "running" | "restarting");
    ContainerActionAvailability {
        can_start: matches!(
            state.as_str(),
            "created" | "configured" | "stopped" | "exited"
        ),
        can_stop: running,
        can_restart: running,
        can_kill: running,
        can_pause: running,
        can_unpause: state == "paused",
        can_remove: matches!(
            state.as_str(),
            "exited" | "stopped" | "created" | "configured" | "dead"
        ),
        can_use_live_tools: running || state == "paused",
        can_exec: running,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PodActionAvailability {
    pub can_start: bool,
    pub can_stop: bool,
    pub can_restart: bool,
    pub can_pause: bool,
    pub can_unpause: bool,
    /// `podman pod rm` needs a stopped pod.
    pub can_remove: bool,
}

/// Degraded pods keep their workloads running but lose the infra container, so
/// only start/stop-style actions are offered and removal waits for a stop.
pub fn pod_action_availability(status: &str) -> PodActionAvailability {
    let status = status.trim().to_ascii_lowercase();
    let running = matches!(status.as_str(), "running" | "degraded");
    PodActionAvailability {
        can_start: matches!(
            status.as_str(),
            "exited" | "stopped" | "created" | "degraded"
        ),
        can_stop: running,
        can_restart: running,
        can_pause: status == "running",
        can_unpause: status == "paused",
        can_remove: matches!(status.as_str(), "exited" | "stopped" | "created"),
    }
}

pub fn container_action_command(
    os_type: &str,
    instance: &ContainerInstance,
    action: ContainerAction,
    container_id: &str,
) -> Result<String, String> {
    let container_id = validated_container_id(container_id)?;
    if !matches!(
        action,
        ContainerAction::Start
            | ContainerAction::Stop
            | ContainerAction::Restart
            | ContainerAction::Kill
            | ContainerAction::Remove
            | ContainerAction::Pause
            | ContainerAction::Unpause
    ) {
        return Err("Unsupported container action.".to_string());
    }
    if matches!(action, ContainerAction::Pause | ContainerAction::Unpause)
        && !instance.engine.supports_pause()
    {
        return Err("This engine does not support pausing containers.".to_string());
    }
    let operation = match action {
        ContainerAction::Remove => instance.engine.container_remove_command(),
        other => other.id(),
    };
    Ok(wrapped_command(
        os_type,
        format!(
            "{} {operation} {container_id}",
            instance.cli_prefix_for(os_type)
        ),
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageAction {
    Remove,
    /// Dangling (untagged) images only.
    Prune,
}

impl ImageAction {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Remove => "sidebar.host_container.actions.remove",
            Self::Prune => "sidebar.host_container.actions.prune",
        }
    }

    pub fn is_destructive(self) -> bool {
        true
    }
}

pub fn image_action_command(
    os_type: &str,
    instance: &ContainerInstance,
    action: ImageAction,
    image_id: Option<&str>,
) -> Result<String, String> {
    let prefix = instance.cli_prefix_for(os_type);
    let command = match action {
        ImageAction::Remove => {
            let reference = validated_resource_name(image_id.unwrap_or_default())?;
            format!("{prefix} rmi {}", quote_argument(os_type, reference))
        }
        ImageAction::Prune => format!("{prefix} image prune -f"),
    };
    Ok(wrapped_command(os_type, command))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VolumeAction {
    Remove,
    Prune,
}

impl VolumeAction {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Remove => "sidebar.host_container.actions.remove",
            Self::Prune => "sidebar.host_container.actions.prune",
        }
    }

    pub fn is_destructive(self) -> bool {
        true
    }
}

pub fn volume_action_command(
    os_type: &str,
    instance: &ContainerInstance,
    action: VolumeAction,
    name: Option<&str>,
) -> Result<String, String> {
    let prefix = instance.cli_prefix_for(os_type);
    let command = match action {
        VolumeAction::Remove => {
            let name = validated_resource_name(name.unwrap_or_default())?;
            format!("{prefix} volume rm {}", shell_quote_single(name))
        }
        VolumeAction::Prune => format!("{prefix} volume prune -f"),
    };
    Ok(wrapped_command(os_type, command))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkAction {
    Remove,
    Prune,
}

impl NetworkAction {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Remove => "sidebar.host_container.actions.remove",
            Self::Prune => "sidebar.host_container.actions.prune",
        }
    }

    pub fn is_destructive(self) -> bool {
        true
    }
}

pub fn network_action_command(
    os_type: &str,
    instance: &ContainerInstance,
    action: NetworkAction,
    name: Option<&str>,
) -> Result<String, String> {
    let prefix = instance.cli_prefix_for(os_type);
    let command = match action {
        NetworkAction::Remove => {
            let name = validated_resource_name(name.unwrap_or_default())?;
            format!("{prefix} network rm {}", shell_quote_single(name))
        }
        NetworkAction::Prune => format!("{prefix} network prune -f"),
    };
    Ok(wrapped_command(os_type, command))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PodAction {
    Start,
    Stop,
    Restart,
    Pause,
    Unpause,
    Remove,
}

impl PodAction {
    pub fn id(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::Remove => "remove",
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Start => "sidebar.host_container.actions.start",
            Self::Stop => "sidebar.host_container.actions.stop",
            Self::Restart => "sidebar.host_container.actions.restart",
            Self::Pause => "sidebar.host_container.actions.pause",
            Self::Unpause => "sidebar.host_container.actions.unpause",
            Self::Remove => "sidebar.host_container.actions.remove",
        }
    }

    pub fn is_destructive(self) -> bool {
        matches!(self, Self::Remove)
    }
}

pub fn pod_action_command(
    os_type: &str,
    instance: &ContainerInstance,
    action: PodAction,
    pod_id: &str,
) -> Result<String, String> {
    if !instance.engine.supports_pods() {
        return Err("This engine does not support pods.".to_string());
    }
    let pod_id = validated_resource_name(pod_id)?;
    let operation = match action {
        PodAction::Remove => "rm",
        other => other.id(),
    };
    Ok(wrapped_command(
        os_type,
        format!(
            "{} pod {operation} {pod_id}",
            instance.cli_prefix_for(os_type)
        ),
    ))
}

/// Read-only outputs rendered in the shared text dialog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContainerCapture {
    ContainerInspect {
        container_id: String,
    },
    ContainerLogs {
        container_id: String,
    },
    ImageInspect {
        image_id: String,
    },
    ImageHistory {
        image_id: String,
    },
    /// OCI/Docker manifest of an image reference, fetched from the registry.
    ManifestInspect {
        reference: String,
    },
    /// One-shot `system df`: images, containers and volumes disk usage.
    SystemDf,
    VolumeInspect {
        name: String,
    },
    VolumeUsage {
        name: String,
    },
    NetworkInspect {
        name: String,
    },
    PodInspect {
        pod_id: String,
    },
    Top {
        container_id: String,
    },
    Diff {
        container_id: String,
    },
}

pub fn capture_command(
    os_type: &str,
    instance: &ContainerInstance,
    capture: &ContainerCapture,
) -> Result<String, String> {
    let prefix = instance.cli_prefix_for(os_type);
    let command = match capture {
        ContainerCapture::ContainerInspect { container_id } => {
            let id = validated_container_id(container_id)?;
            format!("{prefix} inspect {id}")
        }
        ContainerCapture::ContainerLogs { container_id } => {
            let id = validated_container_id(container_id)?;
            format!("{prefix} logs --tail 200 --timestamps {id}")
        }
        ContainerCapture::ImageInspect { image_id } => {
            let id = validated_container_id(image_id)?;
            format!("{prefix} image inspect {id}")
        }
        ContainerCapture::SystemDf => {
            if !instance.engine.supports_system_df() {
                return Err("This engine does not support system df.".to_string());
            }
            format!("{prefix} system df")
        }
        ContainerCapture::ManifestInspect { reference } => {
            if !instance.engine.supports_manifest_inspect() {
                return Err("This engine does not support manifest inspection.".to_string());
            }
            let reference = validated_resource_name(reference)?;
            // Docker gates `manifest` behind the experimental CLI switch;
            // setting it is harmless on versions that enable it by default.
            let experimental = match instance.engine {
                ContainerEngine::Docker => "DOCKER_CLI_EXPERIMENTAL=enabled ",
                _ => "",
            };
            let command = format!(
                "{experimental}{prefix} manifest inspect {}",
                shell_quote_single(reference)
            );
            // The env assignment is POSIX; a Windows capture runs through
            // cmd/PowerShell, so hand it to the Git Bash bridge.
            if crate::container::quote::is_windows(os_type) {
                crate::container::windows::posix_via_bash(
                    &command,
                    "MANIFEST",
                    "Git Bash (bash.exe) not found",
                )
            } else {
                command
            }
        }
        ContainerCapture::ImageHistory { image_id } => {
            if !instance.engine.supports_image_history() {
                return Err("This engine does not support image history.".to_string());
            }
            let id = validated_container_id(image_id)?;
            format!("{prefix} history {id}")
        }
        ContainerCapture::VolumeInspect { name } => {
            let name = validated_resource_name(name)?;
            format!("{prefix} volume inspect {}", shell_quote_single(name))
        }
        ContainerCapture::NetworkInspect { name } => {
            let name = validated_resource_name(name)?;
            format!("{prefix} network inspect {}", shell_quote_single(name))
        }
        ContainerCapture::VolumeUsage { name } => {
            // `wslc` only accepts `--format json|table`, so the Go template
            // below would be rejected instead of listing containers.
            if instance.engine == ContainerEngine::Wslc {
                return Err("Volume usage is not supported on this engine.".to_string());
            }
            let name = validated_resource_name(name)?;
            format!(
                "{prefix} ps -a --filter volume={} --format '{{{{.Names}}}}'",
                shell_quote_single(name)
            )
        }
        ContainerCapture::PodInspect { pod_id } => {
            if !instance.engine.supports_pods() {
                return Err("This engine does not support pods.".to_string());
            }
            let id = validated_resource_name(pod_id)?;
            format!("{prefix} pod inspect {id}")
        }
        ContainerCapture::Top { container_id } => {
            if !instance.engine.supports_container_inspection() {
                return Err("This engine does not support container top.".to_string());
            }
            let id = validated_container_id(container_id)?;
            format!("{prefix} top {id}")
        }
        ContainerCapture::Diff { container_id } => {
            if !instance.engine.supports_container_inspection() {
                return Err("This engine does not support container diff.".to_string());
            }
            let id = validated_container_id(container_id)?;
            format!("{prefix} diff {id}")
        }
    };
    Ok(match os_type {
        "Windows" | "windows" => {
            format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{command} 2>&1\"")
        }
        _ => format!("{command} 2>&1"),
    })
}

/// Interactive terminal command: an interactive shell inside a running
/// container. The terminal owns the stream; nothing is captured or audited.
pub fn exec_shell_command(
    os_type: &str,
    instance: &ContainerInstance,
    container_id: &str,
) -> Result<String, String> {
    let container_id = validated_container_id(container_id)?;
    Ok(format!(
        "{} exec -it {container_id} sh -lc 'if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi'",
        instance.cli_prefix_for(os_type)
    ))
}

/// Interactive terminal command: follow a container's log stream.
pub fn follow_logs_command(
    os_type: &str,
    instance: &ContainerInstance,
    container_id: &str,
) -> Result<String, String> {
    let container_id = validated_container_id(container_id)?;
    Ok(format!(
        "{} logs -f --tail 200 --timestamps {container_id}",
        instance.cli_prefix_for(os_type)
    ))
}

fn wrapped_command(os_type: &str, command: String) -> String {
    match os_type {
        "Windows" | "windows" => {
            format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{command} 2>&1\"")
        }
        _ => format!("{command} 2>&1"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::engine::ContainerEngine;
    use crate::container::transport::ContainerTransport;

    fn instance(engine: ContainerEngine) -> ContainerInstance {
        ContainerInstance::new(
            engine,
            ContainerTransport::Socket {
                endpoint: "unix:///var/run/docker.sock".into(),
            },
            None,
        )
    }

    const ID: &str = "a1b2c3d4e5f6";

    #[test]
    fn container_actions_carry_the_transport_and_engine_remove_verb() {
        let docker = instance(ContainerEngine::Docker);
        assert_eq!(
            container_action_command("Linux", &docker, ContainerAction::Start, ID).unwrap(),
            "docker -H 'unix:///var/run/docker.sock' start a1b2c3d4e5f6 2>&1"
        );
        assert_eq!(
            container_action_command("Linux", &docker, ContainerAction::Remove, ID).unwrap(),
            "docker -H 'unix:///var/run/docker.sock' rm a1b2c3d4e5f6 2>&1"
        );
        let wslc = ContainerInstance::new(ContainerEngine::Wslc, ContainerTransport::Cli, None);
        assert_eq!(
            container_action_command("Windows", &wslc, ContainerAction::Remove, ID).unwrap(),
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"wslc remove a1b2c3d4e5f6 2>&1\""
        );
        assert!(container_action_command("Windows", &wslc, ContainerAction::Pause, ID).is_err());
        assert!(container_action_command("Linux", &docker, ContainerAction::Stop, "abc").is_err());
    }

    #[test]
    fn resource_actions_validate_names_and_prune_flags() {
        let podman = instance(ContainerEngine::Podman);
        assert_eq!(
            volume_action_command("Linux", &podman, VolumeAction::Remove, Some("app-data"))
                .unwrap(),
            "podman --url 'unix:///var/run/docker.sock' volume rm 'app-data' 2>&1"
        );
        assert_eq!(
            network_action_command("Linux", &podman, NetworkAction::Prune, None).unwrap(),
            "podman --url 'unix:///var/run/docker.sock' network prune -f 2>&1"
        );
        assert!(volume_action_command("Linux", &podman, VolumeAction::Remove, None).is_err());
        assert!(
            image_action_command("Linux", &podman, ImageAction::Remove, Some("a1b2c3d4e5f6"))
                .is_ok()
        );
        assert_eq!(
            image_action_command("Linux", &podman, ImageAction::Remove, Some("nginx:latest"))
                .unwrap(),
            "podman --url 'unix:///var/run/docker.sock' rmi 'nginx:latest' 2>&1"
        );
        assert_eq!(
            image_action_command("Linux", &podman, ImageAction::Prune, None).unwrap(),
            "podman --url 'unix:///var/run/docker.sock' image prune -f 2>&1"
        );
    }

    #[test]
    fn pod_actions_require_podman() {
        let podman = instance(ContainerEngine::Podman);
        assert_eq!(
            pod_action_command("Linux", &podman, PodAction::Stop, "pod_ldap").unwrap(),
            "podman --url 'unix:///var/run/docker.sock' pod stop pod_ldap 2>&1"
        );
        assert_eq!(
            pod_action_command("Linux", &podman, PodAction::Remove, "pod_ldap").unwrap(),
            "podman --url 'unix:///var/run/docker.sock' pod rm pod_ldap 2>&1"
        );
        let docker = instance(ContainerEngine::Docker);
        assert!(pod_action_command("Linux", &docker, PodAction::Stop, "pod_ldap").is_err());
    }

    #[test]
    fn captures_cover_every_inventory_and_reject_unsupported_ones() {
        let docker = instance(ContainerEngine::Docker);
        assert_eq!(
            capture_command(
                "Linux",
                &docker,
                &ContainerCapture::ContainerInspect {
                    container_id: ID.into()
                }
            )
            .unwrap(),
            "docker -H 'unix:///var/run/docker.sock' inspect a1b2c3d4e5f6 2>&1"
        );
        assert_eq!(
            capture_command(
                "Linux",
                &docker,
                &ContainerCapture::ContainerLogs {
                    container_id: ID.into()
                }
            )
            .unwrap(),
            "docker -H 'unix:///var/run/docker.sock' logs --tail 200 --timestamps a1b2c3d4e5f6 2>&1"
        );
        let wslc = ContainerInstance::new(ContainerEngine::Wslc, ContainerTransport::Cli, None);
        assert!(
            capture_command(
                "Windows",
                &wslc,
                &ContainerCapture::ImageHistory {
                    image_id: ID.into()
                }
            )
            .is_err()
        );
        assert!(
            capture_command(
                "Windows",
                &wslc,
                &ContainerCapture::VolumeInspect {
                    name: "app-data".into()
                }
            )
            .unwrap()
            .contains("wslc volume inspect 'app-data'")
        );
        assert!(
            capture_command(
                "Linux",
                &docker,
                &ContainerCapture::PodInspect {
                    pod_id: "pod_ldap".into()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn availability_matrices_match_container_states() {
        assert!(container_action_availability("running").can_stop);
        assert!(!container_action_availability("running").can_remove);
        assert!(container_action_availability("exited").can_start);
        assert!(container_action_availability("exited").can_remove);
        assert!(container_action_availability("paused").can_unpause);
        assert!(!container_action_availability("paused").can_exec);
        assert!(container_action_availability("paused").can_use_live_tools);
        assert!(!container_action_availability("dead").can_start);
        assert!(pod_action_availability("degraded").can_stop);
        assert!(!pod_action_availability("degraded").can_remove);
        assert!(pod_action_availability("Exited").can_remove);
    }

    #[test]
    fn terminal_commands_use_the_instance_prefix() {
        let podman = instance(ContainerEngine::Podman);
        assert_eq!(
            exec_shell_command("Linux", &podman, ID).unwrap(),
            "podman --url 'unix:///var/run/docker.sock' exec -it a1b2c3d4e5f6 sh -lc 'if command -v bash >/dev/null 2>&1; then exec bash; else exec sh; fi'"
        );
        assert_eq!(
            follow_logs_command("Linux", &podman, ID).unwrap(),
            "podman --url 'unix:///var/run/docker.sock' logs -f --tail 200 --timestamps a1b2c3d4e5f6"
        );
    }
}

// ---- lifecycle extras ----

pub fn create_volume_command(
    os_type: &str,
    instance: &ContainerInstance,
    name: &str,
) -> Result<String, String> {
    let name = validated_resource_name(name)?;
    Ok(wrapped_command(
        os_type,
        format!(
            "{} volume create {}",
            instance.cli_prefix_for(os_type),
            shell_quote_single(name)
        ),
    ))
}

pub fn create_network_command(
    os_type: &str,
    instance: &ContainerInstance,
    name: &str,
    driver: Option<&str>,
) -> Result<String, String> {
    let name = validated_resource_name(name)?;
    let driver = match driver.map(str::trim).filter(|driver| !driver.is_empty()) {
        Some(driver) => {
            let driver = validated_resource_name(driver)?;
            format!(" --driver {}", shell_quote_single(driver))
        }
        None => String::new(),
    };
    Ok(wrapped_command(
        os_type,
        format!(
            "{} network create{driver} {}",
            instance.cli_prefix_for(os_type),
            shell_quote_single(name)
        ),
    ))
}

pub fn create_pod_command(
    os_type: &str,
    instance: &ContainerInstance,
    name: &str,
) -> Result<String, String> {
    if !instance.engine.supports_pods() {
        return Err("This engine does not support pods.".to_string());
    }
    let name = validated_resource_name(name)?;
    Ok(wrapped_command(
        os_type,
        format!(
            "{} pod create --name {}",
            instance.cli_prefix_for(os_type),
            shell_quote_single(name)
        ),
    ))
}

pub fn rename_container_command(
    os_type: &str,
    instance: &ContainerInstance,
    container_id: &str,
    new_name: &str,
) -> Result<String, String> {
    if !instance.engine.supports_container_maintenance() {
        return Err("This engine does not support renaming containers.".to_string());
    }
    let container_id = validated_container_id(container_id)?;
    let new_name = validated_resource_name(new_name)?;
    Ok(wrapped_command(
        os_type,
        format!(
            "{} rename {container_id} {}",
            instance.cli_prefix_for(os_type),
            shell_quote_single(new_name)
        ),
    ))
}

pub fn tag_image_command(
    os_type: &str,
    instance: &ContainerInstance,
    image_id: &str,
    reference: &str,
) -> Result<String, String> {
    let image_id = validated_container_id(image_id)?;
    let reference = validated_resource_name(reference)?;
    Ok(wrapped_command(
        os_type,
        format!(
            "{} tag {image_id} {}",
            instance.cli_prefix_for(os_type),
            shell_quote_single(reference)
        ),
    ))
}

pub fn healthcheck_command(
    os_type: &str,
    instance: &ContainerInstance,
    container_id: &str,
) -> Result<String, String> {
    if !instance.engine.supports_healthcheck_action() {
        return Err("This engine does not support running healthchecks.".to_string());
    }
    let container_id = validated_container_id(container_id)?;
    Ok(wrapped_command(
        os_type,
        format!(
            "{} healthcheck run {container_id}",
            instance.cli_prefix_for(os_type)
        ),
    ))
}

/// Dangling images only unless `all` is set, which also drops unused tagged images.
pub fn prune_images_command(
    os_type: &str,
    instance: &ContainerInstance,
    all: bool,
) -> Result<String, String> {
    let suffix = if all { " -a" } else { "" };
    Ok(wrapped_command(
        os_type,
        format!(
            "{} image prune{suffix} -f",
            instance.cli_prefix_for(os_type)
        ),
    ))
}

pub fn logout_command(
    os_type: &str,
    instance: &ContainerInstance,
    registry: &str,
) -> Result<String, String> {
    let registry = validated_resource_name(registry)?;
    Ok(wrapped_command(
        os_type,
        format!(
            "{} logout {}",
            instance.cli_prefix_for(os_type),
            shell_quote_single(registry)
        ),
    ))
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::container::engine::ContainerEngine;
    use crate::container::transport::ContainerTransport;

    fn podman() -> ContainerInstance {
        ContainerInstance::new(
            ContainerEngine::Podman,
            ContainerTransport::Socket {
                endpoint: "unix:///run/user/1000/podman/podman.sock".into(),
            },
            None,
        )
    }

    fn wslc() -> ContainerInstance {
        ContainerInstance::new(ContainerEngine::Wslc, ContainerTransport::Cli, None)
    }

    #[test]
    fn creation_commands_quote_names_and_drivers() {
        assert_eq!(
            create_volume_command("Linux", &podman(), "app data").is_err(),
            true
        );
        assert_eq!(
            create_volume_command("Linux", &podman(), "app-data").unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' volume create 'app-data' 2>&1"
        );
        assert_eq!(
            create_network_command("Linux", &podman(), "front", Some("bridge")).unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' network create --driver 'bridge' 'front' 2>&1"
        );
        assert_eq!(
            create_pod_command("Linux", &podman(), "web").unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' pod create --name 'web' 2>&1"
        );
        assert!(create_pod_command("Linux", &wslc(), "web").is_err());
    }

    #[test]
    fn maintenance_commands_validate_identifiers_and_capabilities() {
        assert_eq!(
            rename_container_command("Linux", &podman(), "a1b2c3d4e5f6", "web").unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' rename a1b2c3d4e5f6 'web' 2>&1"
        );
        assert!(rename_container_command("Linux", &wslc(), "a1b2c3d4e5f6", "web").is_err());
        assert_eq!(
            tag_image_command("Linux", &podman(), "a1b2c3d4e5f6", "registry/web:1").unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' tag a1b2c3d4e5f6 'registry/web:1' 2>&1"
        );
        assert!(tag_image_command("Linux", &podman(), "a1b2c3d4e5f6", "bad name").is_err());
        assert!(healthcheck_command("Linux", &wslc(), "a1b2c3d4e5f6").is_err());
        assert_eq!(
            prune_images_command("Linux", &podman(), true).unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' image prune -a -f 2>&1"
        );
        assert_eq!(
            logout_command("Linux", &podman(), "ghcr.io").unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' logout 'ghcr.io' 2>&1"
        );
    }

    #[test]
    fn manifest_inspect_follows_engine_support() {
        let docker = ContainerInstance::new(
            ContainerEngine::Docker,
            ContainerTransport::Socket {
                endpoint: "unix:///run/docker.sock".into(),
            },
            None,
        );
        let manifest = capture_command(
            "Linux",
            &podman(),
            &ContainerCapture::ManifestInspect {
                reference: "docker.io/library/nginx:latest".into(),
            },
        )
        .expect("podman manifest");
        assert!(
            manifest.contains("manifest inspect 'docker.io/library/nginx:latest'"),
            "{manifest}"
        );
        assert!(!manifest.contains("DOCKER_CLI_EXPERIMENTAL"));
        let docker_manifest = capture_command(
            "Linux",
            &docker,
            &ContainerCapture::ManifestInspect {
                reference: "nginx:latest".into(),
            },
        )
        .expect("docker manifest");
        assert!(
            docker_manifest.starts_with("DOCKER_CLI_EXPERIMENTAL=enabled "),
            "{docker_manifest}"
        );
        assert!(
            capture_command(
                "Windows",
                &wslc(),
                &ContainerCapture::ManifestInspect {
                    reference: "nginx:latest".into(),
                }
            )
            .is_err()
        );
    }

    #[test]
    fn top_diff_and_usage_captures_respect_capabilities() {
        assert_eq!(
            capture_command(
                "Linux",
                &podman(),
                &ContainerCapture::Top {
                    container_id: "a1b2c3d4e5f6".into()
                }
            )
            .unwrap(),
            "podman --url 'unix:///run/user/1000/podman/podman.sock' top a1b2c3d4e5f6 2>&1"
        );
        assert!(
            capture_command(
                "Linux",
                &podman(),
                &ContainerCapture::VolumeUsage {
                    name: "app-data".into()
                }
            )
            .unwrap()
            .contains("ps -a --filter volume='app-data'")
        );
        // `wslc` only accepts `--format json|table`, so the Go template is
        // rejected up front instead of failing on the host.
        assert_eq!(
            capture_command(
                "Windows",
                &wslc(),
                &ContainerCapture::VolumeUsage {
                    name: "app-data".into()
                }
            )
            .unwrap_err(),
            "Volume usage is not supported on this engine."
        );
    }
}

#[cfg(test)]
mod run_option_tests {
    use super::*;

    fn sample_options() -> ContainerRunOptions {
        ContainerRunOptions {
            name: "web".to_string(),
            detached: true,
            interactive: true,
            auto_remove: false,
            ports: vec!["8080:80".to_string(), "53:53/udp".to_string()],
            volumes: vec!["/srv/data:/data:ro".to_string()],
            command: "/bin/sh -lc \"env\"".to_string(),
        }
    }

    #[test]
    fn run_arguments_compose_flags_specs_and_command() {
        let args = run_arguments(
            "Linux",
            "nginx:alpine",
            &sample_options(),
            shell_quote_single,
        )
        .expect("args");
        assert_eq!(
            args,
            "-d -it --name 'web' -p '8080:80' -p '53:53/udp' -v '/srv/data:/data:ro' 'nginx:alpine' '/bin/sh' '-lc' 'env'"
        );
    }

    #[test]
    fn run_arguments_omit_empty_optional_fields() {
        let args = run_arguments(
            "Linux",
            "alpine",
            &ContainerRunOptions {
                detached: true,
                ..ContainerRunOptions::default()
            },
            shell_quote_single,
        )
        .expect("args");
        assert_eq!(args, "-d 'alpine'");
    }

    #[test]
    fn run_option_specs_are_validated() {
        assert!(parse_run_options("127.0.0.1:8080:80, [::1]:5353:53/udp", "").is_ok());
        assert!(parse_run_options("8080:80/icmp", "").is_err());
        assert!(parse_run_options("host:8080:80", "").is_err());
        assert!(parse_run_options("", "no-colon-mount").is_err());
        // A crafted spec must not be able to leave the quoted argument.
        assert!(parse_run_options("8080:80', rm -rf /", "").is_err());
        let mut broken = sample_options();
        broken.name = "--all".to_string();
        assert!(run_arguments("Linux", "alpine", &broken, shell_quote_single).is_err());
    }
}
