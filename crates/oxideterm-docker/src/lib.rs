// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Container runtime access for local Docker-compatible engines and Kubernetes pods.
//!
//! Owns engine discovery, container/pod listing, and the normalized exec duplex
//! stream with resize support consumed by the terminal session layer. Does not
//! own any UI, persistence, or session lifecycle beyond the returned transport.

pub mod engine;
pub mod exec;
pub mod list;
pub mod types;

pub use engine::{discover_engines, discover_kube_engines};
pub use exec::{ContainerExecTransport, DEFAULT_SHELL_CANDIDATES, open_container_exec};
pub use list::{inspect_container, list_containers};
pub use types::{
    ContainerConnectionConfig, ContainerDetails, ContainerEntry, ContainerError,
    ContainerLaunchMode, ContainerMount, ContainerNetwork, ContainerPort, EngineInfo, EngineKind,
    EngineTarget, PodContainerInfo,
};
