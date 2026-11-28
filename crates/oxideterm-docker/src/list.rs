// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Container and pod listing for the connection form.

use k8s_openapi::api::core::v1::Pod;
use kube::api::{ListParams, ResourceExt};
use std::path::Path;

use crate::types::{
    ContainerDetails, ContainerEntry, ContainerError, ContainerMount, ContainerNetwork,
    ContainerPort, EngineInfo, EngineKind, EngineTarget, PodContainerInfo,
};

/// Lists containers (all states) for docker-compatible engines, or pods for
/// the kube surface. Kube scoping mirrors kubectl: contexts without a
/// declared namespace address the "default" namespace only.
pub async fn list_containers(target: &EngineTarget) -> Result<Vec<ContainerEntry>, ContainerError> {
    match target {
        EngineTarget::DockerSocket { socket_path } => list_docker_containers(socket_path).await,
        EngineTarget::Kube { context, namespace } => {
            list_kube_pods(context.as_deref(), namespace).await
        }
    }
}

/// Docker reports states in lowercase ("running", "exited") while kube
/// phases arrive capitalized; the table shows one uniform leading-capital
/// spelling, and every consumer compares case-insensitively.
fn capitalize_state(state: String) -> String {
    let mut chars = state.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => state,
    }
}

async fn list_docker_containers(socket_path: &str) -> Result<Vec<ContainerEntry>, ContainerError> {
    let docker = crate::engine::connect_daemon(socket_path)?;
    let options = bollard::query_parameters::ListContainersOptions {
        all: true,
        ..Default::default()
    };
    let summaries = docker
        .list_containers(Some(options))
        .await
        .map_err(|error| ContainerError::Engine(error.to_string()))?;
    // Rebuild the engine identity the discovery picker showed; a raw socket
    // path alone would collapse OrbStack/Colima/Podman into a generic label
    // and make multi-engine tab titles ambiguous.
    let socket_path = Path::new(socket_path);
    let candidate = crate::engine::docker_socket_engine(socket_path);
    let (kind, label) = candidate.unwrap_or((EngineKind::Docker, "Docker".to_string()));
    let engine = EngineInfo {
        kind,
        label,
        socket_path: Some(socket_path.display().to_string()),
        kube_context: None,
        kube_context_namespace: None,
    };
    Ok(summaries
        .into_iter()
        .filter_map(|summary| {
            let id = summary.id?;
            let name = summary
                .names
                .as_ref()
                .and_then(|names| names.first())
                // Docker prefixes the primary name with "/" in list output.
                .map(|name| name.trim_start_matches('/').to_string())
                .or_else(|| id.get(0..12).map(str::to_string))?;
            Some(ContainerEntry {
                engine: engine.clone(),
                id,
                name,
                image: summary.image.unwrap_or_default(),
                state: summary
                    .state
                    .map(|state| capitalize_state(state.to_string()))
                    .unwrap_or_default(),
                namespace: None,
                containers: Vec::new(),
            })
        })
        .collect())
}

async fn kube_client(context: Option<&str>) -> Result<(kube::Client, EngineInfo), ContainerError> {
    let (client, cluster_host) = crate::engine::kube_client_for(context).await?;
    let engine = match context {
        // An explicit context binds the whole surface: label, listing and
        // exec all resolve through that kubeconfig entry.
        Some(context) => kube_context_engine(context)?,
        None => EngineInfo {
            kind: EngineKind::Kube,
            // The engine picker label mirrors discovery: loopback clusters fall
            // back to the kubeconfig cluster name, and no credential-bearing
            // config detail is echoed.
            label: crate::engine::kube_engine_label(
                cluster_host.as_deref(),
                crate::engine::current_kubeconfig_cluster_name().as_deref(),
            ),
            socket_path: None,
            kube_context: None,
            // The no-context surface scopes like kubectl: the current
            // context's default namespace, when one is declared.
            kube_context_namespace: crate::engine::current_kubeconfig_context_namespace(),
        },
    };
    Ok((client, engine))
}

/// Rebuilds the discovery EngineInfo for one explicit context so listing rows
/// carry the same identity the form's engine picker showed.
fn kube_context_engine(context: &str) -> Result<EngineInfo, ContainerError> {
    crate::engine::kube_context_engine_info(&crate::engine::kube_context_candidates(), context)
        .ok_or_else(|| ContainerError::KubeConfigUnavailable)
}

async fn list_kube_pods(
    context: Option<&str>,
    target_namespace: &str,
) -> Result<Vec<ContainerEntry>, ContainerError> {
    let (client, engine) = kube_client(context).await?;
    // Kubectl contract: a context without a declared namespace addresses the
    // "default" namespace, never the whole cluster.
    let namespace = if target_namespace.is_empty() {
        "default"
    } else {
        target_namespace
    };
    let pods = list_pods(&client, namespace)
        .await
        .map_err(|error| ContainerError::Engine(error.to_string()))?;
    Ok(pods
        .into_iter()
        .map(|pod| {
            let name = pod.name_any();
            let namespace = pod.namespace().unwrap_or_else(|| namespace.to_string());
            let image = pod
                .spec
                .as_ref()
                .and_then(|spec| spec.containers.first())
                .and_then(|container| container.image.clone())
                .unwrap_or_default();
            // Phases arrive capitalized from the API; docker states are
            // normalized to the same spelling so the state column stays
            // uniform across engines.
            let state = pod
                .status
                .as_ref()
                .and_then(|status| status.phase.clone())
                .unwrap_or_default();
            // Spec-order containers back the multi-container picker; per
            // container states use the same capitalized spelling.
            let container_statuses = pod
                .status
                .as_ref()
                .and_then(|status| status.container_statuses.clone())
                .unwrap_or_default();
            let containers = pod
                .spec
                .as_ref()
                .map(|spec| {
                    spec.containers
                        .iter()
                        .map(|container| {
                            let state = container_statuses
                                .iter()
                                .find(|status| status.name == container.name)
                                .and_then(|status| status.state.as_ref())
                                .map(|state| {
                                    if state.running.is_some() {
                                        "Running"
                                    } else if state.terminated.is_some() {
                                        "Terminated"
                                    } else {
                                        state
                                            .waiting
                                            .as_ref()
                                            .and_then(|waiting| waiting.reason.as_deref())
                                            .unwrap_or("Waiting")
                                    }
                                })
                                .unwrap_or_default()
                                .to_string();
                            PodContainerInfo {
                                name: container.name.clone(),
                                image: container.image.clone().unwrap_or_default(),
                                state,
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            ContainerEntry {
                engine: engine.clone(),
                id: name.clone(),
                name,
                image,
                state,
                namespace: Some(namespace),
                containers,
            }
        })
        .collect())
}

/// Lists pods in one namespace; the kubectl-strict caller always names the
/// namespace, so the all-namespaces API surface stays out of this crate.
async fn list_pods(client: &kube::Client, namespace: &str) -> Result<Vec<Pod>, kube::Error> {
    let api: kube::Api<Pod> = kube::Api::namespaced(client.clone(), namespace);
    Ok(api.list(&ListParams::default()).await?.items)
}

/// Reads the feature parameters of one container (volume mappings, published
/// ports, networks) shown in the connection form after a selection. The pin
/// selects which container of a multi-container pod the details describe;
/// Docker containers are already the target themselves.
pub async fn inspect_container(
    target: &EngineTarget,
    container_id: &str,
    pod_container: Option<&str>,
) -> Result<ContainerDetails, ContainerError> {
    match target {
        EngineTarget::DockerSocket { socket_path } => {
            inspect_docker_container(socket_path, container_id).await
        }
        EngineTarget::Kube { context, namespace } => {
            inspect_kube_pod(context.as_deref(), namespace, container_id, pod_container).await
        }
    }
}

async fn inspect_docker_container(
    socket_path: &str,
    container_id: &str,
) -> Result<ContainerDetails, ContainerError> {
    let docker = crate::engine::connect_daemon(socket_path)?;
    let response = docker
        .inspect_container(container_id, None)
        .await
        .map_err(|error| ContainerError::Engine(error.to_string()))?;
    let mounts = response
        .mounts
        .unwrap_or_default()
        .into_iter()
        .filter_map(|mount| {
            Some(ContainerMount {
                source: mount.source?,
                destination: mount.destination?,
            })
        })
        .collect();
    let ports = response
        .network_settings
        .as_ref()
        .and_then(|settings| settings.ports.as_ref())
        .map(|ports| {
            ports
                .iter()
                .filter_map(|(key, bindings)| {
                    // Key format is "<port>/<protocol>".
                    let (container_port, protocol) = key.split_once('/')?;
                    let container_port = container_port.parse().ok()?;
                    Some(
                        bindings
                            .clone()
                            .unwrap_or_default()
                            .into_iter()
                            .map(move |binding| ContainerPort {
                                host_ip: binding.host_ip,
                                host_port: binding.host_port,
                                container_port,
                                protocol: protocol.to_string(),
                            }),
                    )
                })
                .flatten()
                .collect()
        })
        .unwrap_or_default();
    let networks = response
        .network_settings
        .as_ref()
        .and_then(|settings| settings.networks.as_ref())
        .map(|networks| {
            networks
                .iter()
                .map(|(name, endpoint)| ContainerNetwork {
                    name: name.clone(),
                    ip: endpoint.ip_address.clone(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(ContainerDetails {
        mounts,
        ports,
        networks,
        main_ip: None,
    })
}

async fn inspect_kube_pod(
    context: Option<&str>,
    namespace: &str,
    pod_name: &str,
    pod_container: Option<&str>,
) -> Result<ContainerDetails, ContainerError> {
    let (client, _) = kube_client(context).await?;
    let pods: kube::Api<Pod> = kube::Api::namespaced(client, namespace);
    let pod = pods
        .get(pod_name)
        .await
        .map_err(|error| ContainerError::Engine(error.to_string()))?;
    let spec = pod.spec.as_ref();
    let volumes = spec
        .map(|spec| spec.volumes.clone().unwrap_or_default())
        .unwrap_or_default();
    // Merge specs only when no container is pinned; a pinned pod describes
    // that container so sidecar mounts and ports stay out of the details. A
    // pin that names no container is stale (renamed or removed since the
    // profile was written), so the whole pod is described instead of nothing.
    let pinned = pod_container.is_some_and(|name| {
        spec.is_some_and(|spec| {
            spec.containers
                .iter()
                .any(|container| container.name == name)
        })
    });
    let in_scope = |container: &&k8s_openapi::api::core::v1::Container| {
        !pinned || pod_container == Some(container.name.as_str())
    };
    // Several containers may mount the same volume (the auto-injected
    // kube-api-access volume sits in every container), so the merged view
    // keeps each distinct mapping once.
    let mut mounts = Vec::new();
    for container in spec
        .iter()
        .flat_map(|spec| spec.containers.iter())
        .filter(in_scope)
    {
        for mount in container.volume_mounts.clone().unwrap_or_default() {
            // Docker mounts show the host source path; pods surface the
            // backing volume name instead.
            let Some(volume) = volumes.iter().find(|volume| volume.name == mount.name) else {
                continue;
            };
            let entry = ContainerMount {
                source: volume.name.clone(),
                destination: mount.mount_path,
            };
            if !mounts.contains(&entry) {
                mounts.push(entry);
            }
        }
    }
    let mut ports = Vec::new();
    for container in spec
        .iter()
        .flat_map(|spec| spec.containers.iter())
        .filter(in_scope)
    {
        for port in container.ports.clone().unwrap_or_default() {
            let Ok(container_port) = u16::try_from(port.container_port) else {
                continue;
            };
            let entry = ContainerPort {
                host_ip: port.host_ip,
                host_port: port.host_port.map(|port| port.to_string()),
                container_port,
                protocol: port.protocol.unwrap_or_else(|| "TCP".to_string()),
            };
            if !ports.contains(&entry) {
                ports.push(entry);
            }
        }
    }
    let main_ip = pod
        .status
        .as_ref()
        .and_then(|status| status.pod_ip.clone())
        .filter(|ip| !ip.is_empty());
    Ok(ContainerDetails {
        mounts,
        ports,
        networks: Vec::new(),
        main_ip,
    })
}
