// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Local engine discovery over docker-compatible sockets and kubeconfig.
//!
//! Discovery is filesystem-only and synchronous so the connection form can
//! populate an engine list without any daemon round trip; liveness is proven
//! separately by [`list::list_containers`] or the exec dial.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::types::{ContainerError, EngineInfo, EngineKind};

/// Named pipe exposed by Docker Desktop's engine on Windows.
#[cfg(windows)]
const WINDOWS_DOCKER_NPIPE: &str = r"\\.\pipe\docker_engine";

pub(crate) const BOLLARD_CONNECT_TIMEOUT: u64 = 60;

/// Probes transport permissions before dialing. Opening the unix socket inode
/// or Windows named pipe with read/write fails with EACCES for restricted
/// endpoints long before hyper reports an opaque "client error (Connect)";
/// other open failures (ENXIO for a live unix socket, missing daemon) are
/// left to the dial itself.
pub(crate) fn ensure_transport_accessible(transport_path: &str) -> Result<(), ContainerError> {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(transport_path)
    {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            Err(ContainerError::AccessDenied)
        }
        Err(_) => Ok(()),
    }
}

/// Connects a bollard handle to the docker-compatible daemon addressed by the
/// engine target's transport path: a unix socket on unix targets, Docker
/// Desktop's named pipe on Windows.
pub(crate) fn connect_daemon(transport_path: &str) -> Result<bollard::Docker, ContainerError> {
    ensure_transport_accessible(transport_path)?;
    #[cfg(unix)]
    let handle = bollard::Docker::connect_with_unix(
        transport_path,
        BOLLARD_CONNECT_TIMEOUT,
        bollard::API_DEFAULT_VERSION,
    );
    #[cfg(windows)]
    let handle = bollard::Docker::connect_with_named_pipe(
        transport_path,
        BOLLARD_CONNECT_TIMEOUT,
        bollard::API_DEFAULT_VERSION,
    );
    handle.map_err(|error| ContainerError::Engine(error.to_string()))
}

/// Probe input kept crate-private so unit tests can assert candidate order.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(unix)]
pub(crate) struct SocketCandidate {
    kind: EngineKind,
    label: String,
    path: PathBuf,
}

/// Ordered socket candidates. `DOCKER_HOST` wins because the user pointed the
/// CLI convention at a specific engine; the well-known paths follow. Results
/// are deduplicated by canonical path so the Docker Desktop symlink in
/// `/var/run/docker.sock` does not shadow the OrbStack/Colima engine behind it.
#[cfg(unix)]
pub(crate) fn socket_candidates(
    home: &Path,
    xdg_runtime: Option<&Path>,
    docker_host: Option<&str>,
) -> Vec<SocketCandidate> {
    let mut candidates = Vec::new();
    if let Some(host) = docker_host
        && let Some(path) = host.strip_prefix("unix://")
        && !path.trim().is_empty()
    {
        candidates.push(SocketCandidate {
            kind: EngineKind::Docker,
            label: "DOCKER_HOST".to_string(),
            path: PathBuf::from(path),
        });
    }
    candidates.push(SocketCandidate {
        kind: EngineKind::Docker,
        label: "Docker".to_string(),
        path: PathBuf::from("/var/run/docker.sock"),
    });
    if !home.as_os_str().is_empty() {
        candidates.push(SocketCandidate {
            kind: EngineKind::Docker,
            label: "Docker Desktop".to_string(),
            path: home.join(".docker/run/docker.sock"),
        });
        candidates.push(SocketCandidate {
            kind: EngineKind::Docker,
            label: "OrbStack".to_string(),
            path: home.join(".orbstack/run/docker.sock"),
        });
        if let Ok(entries) = std::fs::read_dir(home.join(".colima")) {
            // Colima keeps one socket per profile; enumerate so non-default
            // profiles stay reachable without extra configuration.
            let mut profiles: Vec<(String, PathBuf)> = entries
                .filter_map(Result::ok)
                .filter(|entry| entry.path().join("docker.sock").is_file())
                .map(|entry| {
                    let label = entry.file_name().to_string_lossy().into_owned();
                    (label, entry.path().join("docker.sock"))
                })
                .collect();
            profiles.sort_by(|a, b| a.0.cmp(&b.0));
            for (label, path) in profiles {
                candidates.push(SocketCandidate {
                    kind: EngineKind::Docker,
                    label: if label == "default" {
                        "Colima".to_string()
                    } else {
                        format!("Colima {label}")
                    },
                    path,
                });
            }
        }
    }
    if let Some(runtime) = xdg_runtime.filter(|runtime| !runtime.as_os_str().is_empty()) {
        candidates.push(SocketCandidate {
            kind: EngineKind::Podman,
            label: "Podman".to_string(),
            path: runtime.join("podman/podman.sock"),
        });
    }
    candidates.push(SocketCandidate {
        kind: EngineKind::Podman,
        label: "Podman".to_string(),
        path: PathBuf::from("/run/podman/podman.sock"),
    });
    candidates
}

/// Rebuilds the discovery identity for one docker-compatible socket so
/// listing rows and tab titles keep the engine the picker showed instead of
/// collapsing every engine into a generic "Docker".
#[cfg(unix)]
pub(crate) fn docker_socket_engine(socket_path: &Path) -> Option<(EngineKind, String)> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let xdg_runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let docker_host = std::env::var("DOCKER_HOST").ok();
    socket_candidates(
        home.as_deref().unwrap_or(Path::new("")),
        xdg_runtime.as_deref(),
        docker_host.as_deref(),
    )
    .into_iter()
    .find(|candidate| candidate.path == socket_path)
    .map(|candidate| (candidate.kind, candidate.label))
}

/// Windows has a single named-pipe engine, so the generic Docker identity
/// always matches discovery.
#[cfg(not(unix))]
pub(crate) fn docker_socket_engine(_socket_path: &Path) -> Option<(EngineKind, String)> {
    None
}

/// Existing and distinct sockets only, preserving candidate priority.
pub fn discover_engines() -> Vec<EngineInfo> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let xdg_runtime = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from);
    let docker_host = std::env::var("DOCKER_HOST").ok();
    discover_engines_with_roots(
        home.as_deref(),
        xdg_runtime.as_deref(),
        docker_host
            .as_deref()
            .filter(|host| !host.trim().is_empty()),
    )
}

fn discover_engines_with_roots(
    home: Option<&Path>,
    xdg_runtime: Option<&Path>,
    docker_host: Option<&str>,
) -> Vec<EngineInfo> {
    // Docker-compatible daemons are reached through unix sockets on unix
    // targets and through Docker Desktop's named pipe on Windows; the pipe
    // only exists while the daemon runs, so discovery doubles as liveness.
    #[cfg(unix)]
    let engines = discover_socket_engines(home, xdg_runtime, docker_host);
    #[cfg(not(unix))]
    let engines: Vec<EngineInfo> = {
        let _ = (home, xdg_runtime, docker_host);
        discover_pipe_engines()
    };
    engines
}

/// Windows discovery: Docker Desktop registers its engine endpoint as a named
/// pipe; opening it read/write succeeds only when the daemon is running.
#[cfg(windows)]
fn discover_pipe_engines() -> Vec<EngineInfo> {
    match std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(WINDOWS_DOCKER_NPIPE)
    {
        Ok(_) => vec![EngineInfo {
            kind: EngineKind::Docker,
            label: "Docker".to_string(),
            socket_path: Some(WINDOWS_DOCKER_NPIPE.to_string()),
            kube_context: None,
            kube_context_namespace: None,
        }],
        Err(_) => Vec::new(),
    }
}

#[cfg(unix)]
fn discover_socket_engines(
    home: Option<&Path>,
    xdg_runtime: Option<&Path>,
    docker_host: Option<&str>,
) -> Vec<EngineInfo> {
    let candidates = socket_candidates(
        home.unwrap_or_else(|| Path::new("")),
        xdg_runtime,
        docker_host,
    );
    let mut seen = HashSet::new();
    let mut engines = Vec::new();
    for candidate in candidates {
        if !candidate.path.exists() {
            continue;
        }
        let identity =
            std::fs::canonicalize(&candidate.path).unwrap_or_else(|_| candidate.path.clone());
        if !seen.insert(identity) {
            continue;
        }
        engines.push(EngineInfo {
            kind: candidate.kind,
            label: candidate.label,
            socket_path: Some(candidate.path.display().to_string()),
            kube_context: None,
            kube_context_namespace: None,
        });
    }
    engines
}

/// Friendly label for a kube surface. Every local desktop cluster (Docker
/// Desktop, kind, minikube, Rancher Desktop) serves the API over loopback, so
/// the host alone would collapse them all into "127.0.0.1"; the current
/// kubeconfig cluster name identifies the distribution instead.
pub fn kube_engine_label(host: Option<&str>, cluster_name: Option<&str>) -> String {
    let host = host.unwrap_or_default();
    if !matches!(host, "127.0.0.1" | "localhost" | "::1" | "[::1]") {
        return if host.is_empty() {
            "K8s".to_string()
        } else {
            host.to_string()
        };
    }
    // "default" (k3s) and "kubernetes" (kubeadm) are generic installer names
    // that identify nothing; fall back to the generic local-cluster label.
    match cluster_name.map(str::trim).filter(|name| !name.is_empty()) {
        Some("docker-desktop") => "Docker Desktop (K8s)".to_string(),
        Some("default" | "kubernetes") | None => "Local K8s".to_string(),
        Some(name) => name.to_string(),
    }
}

/// Resolves the cluster name behind the kubeconfig's current context without
/// touching the network or echoing any credential-bearing config detail.
pub(crate) fn current_kubeconfig_cluster_name() -> Option<String> {
    let kubeconfig = kube::config::Kubeconfig::read().ok()?;
    let context_name = kubeconfig.current_context.as_deref()?;
    let context = kubeconfig
        .contexts
        .iter()
        .find(|entry| entry.name == context_name)?;
    Some(context.context.as_ref()?.cluster.clone())
}

/// Default namespace of the kubeconfig's current context, mirroring kubectl's
/// listing scope for the no-context surface.
pub(crate) fn current_kubeconfig_context_namespace() -> Option<String> {
    let kubeconfig = kube::config::Kubeconfig::read().ok()?;
    let context_name = kubeconfig.current_context.as_deref()?;
    let context = kubeconfig
        .contexts
        .iter()
        .find(|entry| entry.name == context_name)?;
    context
        .context
        .as_ref()?
        .namespace
        .clone()
        .filter(|namespace| !namespace.is_empty())
}

/// Builds the kube client shared by listing and exec/dial. The second tuple
/// element is the cluster API host from the loaded config, which only the
/// listing path needs to label the no-context engine.
pub(crate) async fn kube_client_for(
    context: Option<&str>,
) -> Result<(kube::Client, Option<String>), ContainerError> {
    let config = match kube::config::Kubeconfig::read() {
        Ok(file) => config_for_context(file, context)?,
        Err(_) if context.is_none() => {
            kube::Config::incluster().map_err(|_| ContainerError::KubeConfigUnavailable)?
        }
        Err(_) => return Err(ContainerError::KubeConfigUnavailable),
    };
    let cluster_host = config.cluster_url.host().map(str::to_string);
    let client = kube::Client::try_from(config)
        .map_err(|_| ContainerError::Engine("Could not configure Kubernetes client".into()))?;
    Ok((client, cluster_host))
}

/// Host of a cluster server URL ("https://127.0.0.1:6443" -> "127.0.0.1").
/// Manual parsing keeps discovery free of credential-bearing config detail
/// while matching the loopback labels [`kube_engine_label`] special-cases.
pub(crate) fn kube_server_host(server: &str) -> Option<String> {
    let after_scheme = server.split_once("://").map_or(server, |(_, rest)| rest);
    let authority = after_scheme.split('/').next()?;
    // Userinfo, if any, ends at the last '@' before the host.
    let host_port = authority.rsplit('@').next()?;
    if let Some(rest) = host_port.strip_prefix('[') {
        // Bracketed IPv6 literal; the port follows the closing bracket.
        return rest.split(']').next().map(str::to_string);
    }
    let host = host_port
        .rsplit_once(':')
        .map_or(host_port, |(host, _)| host);
    (!host.is_empty()).then(|| host.to_string())
}

/// One kubeconfig context surfaced as a selectable cluster entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct KubeContextCandidate {
    pub context: String,
    pub cluster: Option<String>,
    pub server_host: Option<String>,
    /// The context's default namespace ("context.namespace" in kubeconfig),
    /// mirroring kubectl: a context bound to a namespace scopes its listing.
    pub namespace: Option<String>,
}

/// Reads every kubeconfig context as a discovery candidate. The current
/// context leads so the default cluster is the first row the form offers.
pub(crate) fn kube_context_candidates() -> Vec<KubeContextCandidate> {
    match kube::config::Kubeconfig::read() {
        Ok(kubeconfig) => kube_context_candidates_from(&kubeconfig),
        // A missing or unreadable kubeconfig is a routine desktop state, not
        // an error worth surfacing next to engine sockets.
        Err(_) => Vec::new(),
    }
}

/// Pure candidate construction over one parsed kubeconfig.
fn kube_context_candidates_from(
    kubeconfig: &kube::config::Kubeconfig,
) -> Vec<KubeContextCandidate> {
    let current = kubeconfig.current_context.clone().unwrap_or_default();
    let cluster_server = |name: &str| {
        kubeconfig
            .clusters
            .iter()
            .find(|cluster| cluster.name == name)
            .and_then(|cluster| cluster.cluster.as_ref())
            .and_then(|cluster| cluster.server.as_deref())
            .and_then(kube_server_host)
    };
    let mut candidates: Vec<KubeContextCandidate> = kubeconfig
        .contexts
        .iter()
        .filter_map(|entry| {
            let context = entry.context.as_ref()?;
            // A context without a cluster, or pointing at a cluster without a
            // server URL, can never dial anything; skip it instead of
            // surfacing a row that only produces errors.
            let cluster = Some(context.cluster.clone());
            let server_host = cluster_server(&context.cluster)?;
            // A named user reference must exist; no reference keeps anonymous
            // access (some control planes allow it).
            let user_resolves = context
                .user
                .as_deref()
                .map(|user| {
                    kubeconfig
                        .auth_infos
                        .iter()
                        .any(|auth_info| auth_info.name == user)
                })
                .unwrap_or(true);
            if !user_resolves {
                return None;
            }
            Some(KubeContextCandidate {
                context: entry.name.clone(),
                cluster,
                server_host: Some(server_host),
                namespace: context.namespace.clone(),
            })
        })
        .collect();
    // One context per name by kubeconfig contract, but a duplicate would
    // still silently duplicate form rows; keep the first occurrence per name
    // (dedup_by would miss non-adjacent duplicates).
    let mut seen = HashSet::new();
    candidates.retain(|entry| seen.insert(entry.context.clone()));
    if let Some(position) = candidates.iter().position(|entry| entry.context == current) {
        let current_entry = candidates.remove(position);
        candidates.insert(0, current_entry);
    }
    candidates
}

/// Kube surface discovery: one form entry per kubeconfig context. Only reads
/// the local config, no cluster I/O and no credential material in labels.
pub fn discover_kube_engines() -> Vec<EngineInfo> {
    discover_kube_engines_from(&kube_context_candidates())
}

fn discover_kube_engines_from(candidates: &[KubeContextCandidate]) -> Vec<EngineInfo> {
    candidates
        .iter()
        .filter_map(|candidate| kube_context_engine_info(candidates, &candidate.context))
        .collect()
}

// kube's default loader mixes the cluster proxy with HTTPS_PROXY / NO_PROXY.
// The desktop connection uses only the selected cluster's transport settings.
fn config_for_context(
    file: kube::config::Kubeconfig,
    name: Option<&str>,
) -> Result<kube::Config, ContainerError> {
    let missing = || ContainerError::KubeConfigUnavailable;
    let name = name
        .or(file.current_context.as_deref())
        .ok_or_else(missing)?;
    let context = file
        .contexts
        .iter()
        .find(|entry| entry.name == name)
        .and_then(|entry| entry.context.as_ref())
        .ok_or_else(missing)?;
    let cluster = file
        .clusters
        .iter()
        .find(|entry| entry.name == context.cluster)
        .and_then(|entry| entry.cluster.as_ref())
        .ok_or_else(missing)?;
    let info = kube::config::ExecAuthCluster::try_from(cluster).map_err(|_| missing())?;
    let mut config = kube::Config::new(
        cluster
            .server
            .as_deref()
            .ok_or_else(missing)?
            .parse()
            .map_err(|_| missing())?,
    );
    if let Some(namespace) = &context.namespace {
        config.default_namespace = namespace.clone();
    }
    config.accept_invalid_certs = cluster.insecure_skip_tls_verify.unwrap_or_default();
    config.tls_server_name = cluster.tls_server_name.clone();
    config.disable_compression = cluster.disable_compression.unwrap_or_default();
    config.root_cert = info
        .certificate_authority_data
        .as_deref()
        .map(|data| {
            pem::parse_many(data).map(|certs| {
                certs
                    .into_iter()
                    .filter(|p| p.tag() == "CERTIFICATE")
                    .map(|p| p.into_contents())
                    .collect()
            })
        })
        .transpose()
        .map_err(|_| missing())?;
    config.proxy_url = cluster
        .proxy_url
        .as_deref()
        .filter(|url| !url.is_empty())
        .map(str::parse)
        .transpose()
        .map_err(|_| missing())?;
    if let Some(user) = &context.user {
        config.auth_info = file
            .auth_infos
            .into_iter()
            .find(|entry| &entry.name == user)
            .and_then(|entry| entry.auth_info)
            .unwrap_or_default();
        if let Some(exec) = &mut config.auth_info.exec
            && exec.provide_cluster_info
        {
            exec.cluster = Some(info);
        }
    }
    Ok(config)
}

/// Engine entry for one context, shared by discovery and listing so both
/// surfaces render identical labels. Labels that would repeat (two contexts
/// on the same cluster, e.g. per-namespace contexts) get the context name
/// appended to stay distinguishable.
pub(crate) fn kube_context_engine_info(
    candidates: &[KubeContextCandidate],
    context: &str,
) -> Option<EngineInfo> {
    let candidate = candidates.iter().find(|entry| entry.context == context)?;
    let base_label = kube_engine_label(
        candidate.server_host.as_deref(),
        candidate.cluster.as_deref(),
    );
    let repeats = candidates
        .iter()
        .filter(|entry| {
            kube_engine_label(entry.server_host.as_deref(), entry.cluster.as_deref()) == base_label
        })
        .count();
    let label = if repeats > 1 {
        format!("{base_label} [{}]", candidate.context)
    } else {
        base_label
    };
    Some(EngineInfo {
        kind: EngineKind::Kube,
        label,
        socket_path: None,
        kube_context: Some(candidate.context.clone()),
        kube_context_namespace: candidate.namespace.clone(),
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn kube_label_uses_cluster_name_on_loopback_and_host_elsewhere() {
        // The Docker Desktop regression: loopback clusters must never surface
        // as a bare IP label.
        assert_eq!(
            kube_engine_label(Some("127.0.0.1"), Some("docker-desktop")),
            "Docker Desktop (K8s)"
        );
        assert_eq!(
            kube_engine_label(Some("127.0.0.1"), Some("kind-kind")),
            "kind-kind"
        );
        // Generic installer names (k3s "default", kubeadm "kubernetes") carry
        // no identity, so they collapse into the generic local label.
        assert_eq!(
            kube_engine_label(Some("127.0.0.1"), Some("default")),
            "Local K8s"
        );
        assert_eq!(
            kube_engine_label(Some("localhost"), Some("kubernetes")),
            "Local K8s"
        );
        assert_eq!(kube_engine_label(Some("localhost"), None), "Local K8s");
        assert_eq!(
            kube_engine_label(Some("localhost"), Some("  ")),
            "Local K8s"
        );
        assert_eq!(
            kube_engine_label(Some("k8s.example.com"), Some("prod")),
            "k8s.example.com"
        );
        assert_eq!(kube_engine_label(None, None), "K8s");
    }

    #[test]
    fn kube_server_host_strips_scheme_port_and_userinfo() {
        assert_eq!(
            kube_server_host("https://127.0.0.1:6443"),
            Some("127.0.0.1".into())
        );
        assert_eq!(
            kube_server_host("https://k8s.example.com/"),
            Some("k8s.example.com".into())
        );
        assert_eq!(
            kube_server_host("https://user@127.0.0.1:8443"),
            Some("127.0.0.1".into())
        );
        assert_eq!(kube_server_host("https://[::1]:6443"), Some("::1".into()));
        assert_eq!(kube_server_host("garbage"), Some("garbage".into()));
    }

    fn context_fixture(current: &str) -> kube::config::Kubeconfig {
        use kube::config::{Cluster, Context, NamedAuthInfo, NamedCluster, NamedContext};
        let cluster = |name: &str, server: &str| NamedCluster {
            name: name.into(),
            cluster: Some(Cluster {
                server: Some(server.into()),
                ..Default::default()
            }),
            other: Default::default(),
        };
        let context = |name: &str, cluster: &str, user: &str, namespace: &str| NamedContext {
            name: name.into(),
            context: Some(Context {
                cluster: cluster.into(),
                user: Some(user.into()),
                namespace: (!namespace.is_empty()).then(|| namespace.into()),
                ..Default::default()
            }),
            other: Default::default(),
        };
        kube::config::Kubeconfig {
            current_context: Some(current.into()),
            clusters: vec![
                cluster("docker-desktop", "https://127.0.0.1:6443"),
                cluster("prod", "https://k8s.example.com"),
            ],
            auth_infos: vec![
                NamedAuthInfo {
                    name: "desktop-user".into(),
                    auth_info: None,
                    other: Default::default(),
                },
                NamedAuthInfo {
                    name: "prod-user".into(),
                    auth_info: None,
                    other: Default::default(),
                },
            ],
            contexts: vec![
                context("prod-ctx", "prod", "prod-user", ""),
                // References a cluster entry that does not exist; discovery
                // must skip it instead of offering a dead row.
                context("dangling-ctx", "missing-cluster", "desktop-user", ""),
                context("desktop-ctx", "docker-desktop", "desktop-user", ""),
                context("desktop-ns-b", "docker-desktop", "desktop-user", "exp-b"),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn kube_context_candidates_lead_with_the_current_context() {
        let mut file = context_fixture("desktop-ctx");
        file.clusters[1].cluster.as_mut().unwrap().proxy_url =
            Some("http://cluster-proxy:8080".into());
        let config = config_for_context(file.clone(), Some("prod-ctx")).unwrap();
        assert_eq!(
            config.proxy_url.unwrap().to_string(),
            "http://cluster-proxy:8080/"
        );
        assert!(
            config_for_context(file.clone(), None)
                .unwrap()
                .proxy_url
                .is_none()
        );
        let candidates = kube_context_candidates_from(&file);
        assert_eq!(
            candidates
                .iter()
                .map(|entry| entry.context.as_str())
                .collect::<Vec<_>>(),
            vec!["desktop-ctx", "prod-ctx", "desktop-ns-b"],
            "the current context must be the first row and dangling contexts must be skipped"
        );
        // A context-declared namespace carries into the candidate so the
        // listing can scope itself like kubectl.
        assert_eq!(candidates[0].namespace, None);
        assert_eq!(candidates[2].namespace.as_deref(), Some("exp-b"));
        // Loopback clusters resolve their identity from the config cluster
        // name, remote clusters from the host.
        assert_eq!(
            kube_engine_label(
                candidates[0].server_host.as_deref(),
                candidates[0].cluster.as_deref()
            ),
            "Docker Desktop (K8s)"
        );
        assert_eq!(
            kube_engine_label(
                candidates[1].server_host.as_deref(),
                candidates[1].cluster.as_deref()
            ),
            "k8s.example.com"
        );
    }

    #[test]
    fn same_cluster_contexts_get_context_name_appended_to_the_label() {
        let mut file = context_fixture("desktop-ctx");
        file.clusters[1].cluster.as_mut().unwrap().proxy_url =
            Some("http://cluster-proxy:8080".into());
        let config = config_for_context(file.clone(), Some("prod-ctx")).unwrap();
        assert_eq!(
            config.proxy_url.unwrap().to_string(),
            "http://cluster-proxy:8080/"
        );
        assert!(
            config_for_context(file.clone(), None)
                .unwrap()
                .proxy_url
                .is_none()
        );
        let candidates = kube_context_candidates_from(&file);
        let engines = discover_kube_engines_from(&candidates);
        let labels = engines
            .iter()
            .map(|engine| engine.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            labels,
            vec![
                "Docker Desktop (K8s) [desktop-ctx]",
                "k8s.example.com",
                "Docker Desktop (K8s) [desktop-ns-b]",
            ],
            "repeated labels must carry the context name; unique labels stay clean"
        );
        // The listing surface rebuilds the identical label for one context.
        let rebuilt =
            kube_context_engine_info(&candidates, "desktop-ns-b").expect("resolvable context");
        assert_eq!(rebuilt.label, "Docker Desktop (K8s) [desktop-ns-b]");
        assert_eq!(rebuilt.kube_context.as_deref(), Some("desktop-ns-b"));
    }

    fn write_socket(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn candidates_follow_docker_host_and_well_known_priority() {
        let home = std::env::temp_dir().join("oxideterm-docker-test-home");
        let _ = std::fs::remove_dir_all(&home);
        write_socket(&home.join(".orbstack/run/docker.sock"));
        let runtime = std::env::temp_dir().join("oxideterm-docker-test-runtime");
        write_socket(&runtime.join("podman/podman.sock"));

        let candidates = socket_candidates(
            &home,
            Some(&runtime),
            Some("unix:///tmp/oxideterm-custom.sock"),
        );
        let labels: Vec<&str> = candidates.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(
            labels.first(),
            Some(&"DOCKER_HOST"),
            "DOCKER_HOST must lead the candidate order"
        );
        let kinds: Vec<EngineKind> = candidates.iter().map(|c| c.kind).collect();
        assert!(
            kinds.contains(&EngineKind::Podman),
            "podman runtime socket must be a candidate"
        );
        assert!(
            candidates.iter().all(|c| c.path.is_absolute()),
            "all candidates must be absolute paths"
        );
    }

    #[test]
    fn discovery_deduplicates_canonical_paths_and_missing_sockets() {
        let home = std::env::temp_dir().join("oxideterm-docker-test-dedup");
        let _ = std::fs::remove_dir_all(&home);
        let real = home.join(".orbstack/run/docker.sock");
        write_socket(&real);
        // Both well-known paths point at the same engine through a symlink.
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, home.join(".docker/run/docker.sock")).unwrap_or(());

        let engines = discover_engines_with_roots(Some(&home), None, None);
        // The host may legitimately expose unrelated engines (for example
        // /var/run/docker.sock), so assert the invariant instead of a count:
        // two candidates resolving to one canonical path must collapse.
        let identities: HashSet<PathBuf> = engines
            .iter()
            .filter_map(|engine| engine.socket_path.as_deref())
            .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path)))
            .collect();
        assert_eq!(
            engines.len(),
            identities.len(),
            "symlinked duplicates must collapse into one engine, got {engines:?}"
        );
        assert!(
            identities.contains(&std::fs::canonicalize(&real).unwrap()),
            "the orbstack fixture identity must surface as a discovered engine"
        );
    }
}
