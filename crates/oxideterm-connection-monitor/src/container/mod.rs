// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Container backend abstraction shared by the Host Tools container panel.
//!
//! The module is engine-agnostic: [`ContainerEngine`] selects the CLI family
//! (Docker, Podman, the WSL container CLI), [`ContainerTransport`] selects how
//! that CLI reaches its daemon (unix/npipe socket, TCP endpoint, plain CLI),
//! and every command builder produces shell text that runs on the monitored
//! host. Discovery, snapshots and command builders never execute anything
//! locally, so nothing here depends on the workstation having a container CLI.

mod command;
mod discovery;
mod engine;
mod instance;
mod model;
mod parse;
mod quote;
mod registries;
mod snapshot;
mod stats;
mod terminal;
mod transport;
mod windows;

pub use command::{
    ContainerAction, ContainerActionAvailability, ContainerCapture, ContainerRunOptions,
    ImageAction, NetworkAction, PodAction, PodActionAvailability, VolumeAction, capture_command,
    container_action_availability, container_action_command, container_prune_command,
    create_network_command, create_pod_command, create_volume_command, exec_shell_command,
    follow_logs_command, healthcheck_command, image_action_command, logout_command,
    network_action_command, parse_run_options, pod_action_availability, pod_action_command,
    prune_images_command, rename_container_command, run_arguments, tag_image_command,
    volume_action_command,
};
pub use discovery::{
    ContainerDiscovery, ContainerDiscoveryError, ContainerUnavailable, discovery_command,
    parse_discovery,
};
pub use engine::ContainerEngine;
pub use instance::{ContainerInstance, dedupe_instances, preferred_instance};
pub use model::{
    Container, ContainerSnapshot, ContainerStatus, Image, ImageSort, Network, Pod, Volume,
    container_row_signature, container_state_label_key, pod_row_signature, pod_state_label_key,
    sort_images,
};
pub use parse::{parse_snapshot, recover_single_image_manifest, split_json_prefix};
pub use quote::{DEFAULT_REGISTRY, normalized_registry, qualified_image_reference};
pub use registries::{
    ContainerRegistry, ContainerRegistrySource, parse_registries, registries_command,
};
pub use snapshot::snapshot_command;
pub use stats::{ContainerStat, parse_stats, stats_command};
pub use terminal::{ContainerTerminalOp, terminal_command};
pub use transport::{ContainerTransport, ContainerTransportKind};
