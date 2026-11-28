// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Shared DTOs and error surface for container engine access.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineKind {
    Docker,
    Podman,
    Kube,
}

/// One discovered local engine or kubeconfig surface shown in the connection form.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngineInfo {
    pub kind: EngineKind,
    /// Stable, already-localized-free display label (engine name or profile path).
    pub label: String,
    /// Docker-compatible unix socket path; `None` for the kube surface.
    pub socket_path: Option<String>,
    /// Kube only: the kubeconfig context this surface is bound to. Discovery
    /// emits one entry per context so multi-cluster setups can pick a target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kube_context: Option<String>,
    /// Kube only: the context's default namespace. A declared namespace
    /// scopes the pod listing to it, mirroring `kubectl get pods`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kube_context_namespace: Option<String>,
}

/// One container inside a listed pod, in spec order. Mirrors the docker
/// listing's column shape so the multi-container picker can reuse the same
/// table treatment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PodContainerInfo {
    pub name: String,
    pub image: String,
    /// Capitalized status phase ("Running", "Waiting", "Terminated"),
    /// matching the docker state spelling; empty when the pod reports no
    /// status yet.
    pub state: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ContainerEntry {
    pub engine: EngineInfo,
    /// Docker: container id. Kube: pod name.
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    /// Kube only: namespace the pod lives in.
    pub namespace: Option<String>,
    /// Kube only: containers inside the pod, in spec order. Docker entries
    /// leave this empty; pods with more than one entry need the launch to
    /// name which one the exec/attach targets.
    #[serde(default)]
    pub containers: Vec<PodContainerInfo>,
}

impl ContainerEntry {
    /// Kube pods are ambiguous without their namespace; Docker containers are
    /// unique on their engine already.
    pub fn display_label(&self) -> String {
        match &self.namespace {
            Some(namespace) => format!("{}/{}", namespace, self.name),
            None => self.name.clone(),
        }
    }

    /// Listing/inspect/launch target for this row. Kube pods bind the
    /// engine's context and the row's namespace; Docker rows address their
    /// exact socket so a later exec cannot silently hit another engine.
    pub fn launch_target(&self) -> EngineTarget {
        match self.engine.kind {
            EngineKind::Kube => EngineTarget::Kube {
                context: self.engine.kube_context.clone(),
                namespace: self.namespace.clone().unwrap_or_default(),
            },
            _ => EngineTarget::DockerSocket {
                socket_path: self.engine.socket_path.clone().unwrap_or_default(),
            },
        }
    }

    pub fn contains_pod_container(&self, name: &str) -> bool {
        self.containers
            .iter()
            .any(|container| container.name == name)
    }

    /// Only an unpinned launch may default to the first container.
    pub fn normalize_pod_container(&self, pin: Option<&str>) -> Option<String> {
        if let Some(name) = pin {
            return Some(name.to_owned());
        }
        if self.containers.len() <= 1 {
            return None;
        }
        self.containers
            .first()
            .map(|container| container.name.clone())
    }
}

/// Persistable launch target. Docker-compatible engines are addressed by the
/// exact socket they were discovered on so a later exec does not silently
/// attach to a different engine that claimed the default socket path. Kube
/// targets keep the kubeconfig context they were listed with; `None` defers
/// to the config's current context. `namespace` is the namespace the target
/// operates in: the context's default namespace while listing from the form,
/// the pod's own namespace once bound to a launch.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EngineTarget {
    DockerSocket {
        socket_path: String,
    },
    Kube {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        context: Option<String>,
        namespace: String,
    },
}

/// How the terminal process is started inside the container. Exec variants
/// spawn a fresh process; `Attach` binds the terminal to the already-running
/// main process TTY.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContainerLaunchMode {
    /// Interactive shell; `None` lets the backend try its POSIX shell
    /// candidates in order.
    Shell { shell: Option<String> },
    /// Command line executed through the container's POSIX shell.
    Command { command: String },
    /// Attach to the container/pod main process TTY instead of exec.
    Attach,
}

impl std::fmt::Debug for ContainerLaunchMode {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shell { shell } => out.debug_struct("Shell").field("shell", shell).finish(),
            Self::Command { .. } => out.write_str("Command { redacted }"),
            Self::Attach => out.write_str("Attach"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerConnectionConfig {
    pub engine: EngineTarget,
    /// Docker: container id. Kube: pod name.
    pub container_id: String,
    pub label: String,
    /// Pod namespace for display and profile matching; the exec/attach dial
    /// binds through `engine` instead, which carries the same value.
    pub namespace: Option<String>,
    /// Kube only: container inside a pod the exec/attach targets. The form
    /// pins the pod's first container whenever a pod lists more than one, so
    /// a reopened target keeps the same shell; `None` lets the API server
    /// pick a container (its first spec container in practice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pod_container: Option<String>,
    pub launch: ContainerLaunchMode,
}

impl ContainerConnectionConfig {
    /// Stable launch identity for deduplicating live sessions. The display
    /// label is excluded on purpose: the same target opened from the form and
    /// from a saved profile carries different labels but must resolve to one
    /// session.
    pub fn launch_identity(
        &self,
    ) -> (
        &EngineTarget,
        &str,
        Option<&str>,
        Option<&str>,
        &ContainerLaunchMode,
    ) {
        (
            &self.engine,
            self.container_id.as_str(),
            self.namespace.as_deref(),
            self.pod_container.as_deref(),
            &self.launch,
        )
    }
}

/// Container feature parameters surfaced by the connection form after
/// selecting a container: volume mappings, published ports, and networks.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContainerDetails {
    pub mounts: Vec<ContainerMount>,
    pub ports: Vec<ContainerPort>,
    pub networks: Vec<ContainerNetwork>,
    /// Kube only: the pod IP. Docker surfaces per-network addresses instead.
    pub main_ip: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerMount {
    pub source: String,
    pub destination: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerPort {
    pub host_ip: Option<String>,
    pub host_port: Option<String>,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContainerNetwork {
    pub name: String,
    pub ip: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShellFallback {
    /// The daemon reported the executable is absent; try the next candidate.
    MissingExecutable,
    /// Unrelated failure (engine unreachable, exec rejected, ...).
    Other,
}

#[derive(Debug, thiserror::Error)]
pub enum ContainerError {
    #[error("engine unavailable: {0}")]
    Engine(String),
    #[error("access denied to container engine socket")]
    AccessDenied,
    #[error("no usable shell found in container {0}")]
    NoShell(String),
    #[error("kubeconfig unavailable")]
    KubeConfigUnavailable,
    #[error("{0}")]
    Exec(String),
}

/// Classifies a daemon error emitted while starting an exec so shell
/// candidates can fall through without masking unrelated failures.
pub fn classify_shell_start_error(error: &str) -> ShellFallback {
    let lowered = error.to_lowercase();
    let missing = lowered.contains("no such file or directory")
        // Runtime-wrapped variants keep the "not found" tail; the bare OCI
        // marker must not count, or every runtime rejection (e.g. permission
        // denied) would masquerade as a missing shell.
        || lowered.contains("not found");
    if missing {
        ShellFallback::MissingExecutable
    } else {
        ShellFallback::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_error_classification_distinguishes_missing_executables() {
        assert_eq!(
            classify_shell_start_error(
                "exec: \"/bin/bash\": stat /bin/bash: no such file or directory"
            ),
            ShellFallback::MissingExecutable
        );
        assert_eq!(
            classify_shell_start_error(
                "OCI runtime exec failed: exec: \"/bin/bash\": executable file not found in $PATH"
            ),
            ShellFallback::MissingExecutable
        );
        assert_eq!(
            classify_shell_start_error("OCI runtime exec failed: permission denied"),
            ShellFallback::Other,
            "the bare OCI marker must not count as a missing executable"
        );
        assert_eq!(
            classify_shell_start_error("container is not running"),
            ShellFallback::Other
        );
        assert_eq!(
            classify_shell_start_error("cannot connect to daemon"),
            ShellFallback::Other
        );
    }

    #[test]
    fn connection_config_roundtrips_through_serde() {
        let engines = [
            EngineTarget::DockerSocket {
                socket_path: "/var/run/docker.sock".into(),
            },
            EngineTarget::Kube {
                context: Some("kind-kind".into()),
                namespace: "default".into(),
            },
        ];
        // The serialized tag spelling is the standalone-persistence contract.
        let modes = [
            (
                ContainerLaunchMode::Shell { shell: None },
                r#"{"type":"shell","shell":null}"#,
            ),
            (
                ContainerLaunchMode::Command {
                    command: "top -d 1".into(),
                },
                r#"{"type":"command","command":"top -d 1"}"#,
            ),
            (ContainerLaunchMode::Attach, r#"{"type":"attach"}"#),
        ];
        for (index, (mode, encoded_mode)) in modes.iter().enumerate() {
            let config = ContainerConnectionConfig {
                engine: engines[index % engines.len()].clone(),
                container_id: "abc123".into(),
                label: "web".into(),
                namespace: (index == 1).then(|| "default".to_string()),
                pod_container: (index == 1).then(|| "sidecar".to_string()),
                launch: mode.clone(),
            };
            let encoded = serde_json::to_string(&config).unwrap();
            assert!(
                encoded.contains(encoded_mode),
                "mode {index} must serialize with its tagged shape"
            );
            let decoded: ContainerConnectionConfig = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, config);
        }
        // Kube targets without a context decode from the pre-context payload
        // shape and defer to the current context.
        let legacy: EngineTarget =
            serde_json::from_str(r#"{"Kube":{"namespace":"default"}}"#).unwrap();
        assert_eq!(
            legacy,
            EngineTarget::Kube {
                context: None,
                namespace: "default".into()
            }
        );
    }

    /// One launch identity per pod: the pin must not depend on whether it came
    /// from a saved profile, a backfill or a fresh selection, or reconnect
    /// dedup opens a second session for the same target.
    #[test]
    fn pod_container_pin_normalizes_to_one_identity_per_pod() {
        let pod = |containers: &[&str]| ContainerEntry {
            engine: EngineInfo {
                kind: EngineKind::Kube,
                label: "k3s".into(),
                socket_path: None,
                kube_context: Some("k3s".into()),
                kube_context_namespace: Some("default".into()),
            },
            id: "pod-uid".into(),
            name: "web".into(),
            image: String::new(),
            state: "Running".into(),
            namespace: Some("default".into()),
            containers: containers
                .iter()
                .map(|name| PodContainerInfo {
                    name: (*name).into(),
                    image: "alpine".into(),
                    state: "Running".into(),
                })
                .collect(),
        };

        let multi = pod(&["c1", "c2"]);
        assert_eq!(multi.normalize_pod_container(None).as_deref(), Some("c1"));
        assert_eq!(
            multi.normalize_pod_container(Some("c2")).as_deref(),
            Some("c2")
        );
        assert_eq!(
            multi.normalize_pod_container(Some("removed")).as_deref(),
            Some("removed"),
            "never silently retarget an explicit pin"
        );

        let single = pod(&["only"]);
        assert_eq!(
            single.normalize_pod_container(Some("only")),
            Some("only".to_owned()),
            "keep an explicit pin even when only one container remains"
        );
        assert_eq!(single.normalize_pod_container(None), None);
    }
}
