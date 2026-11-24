// Copyright (C) 2026 jaime.su
// SPDX-License-Identifier: GPL-3.0-only

//! Unified inventory shapes rendered by the container panel.
//!
//! Every engine contributes the same rows so the panel never branches on the
//! backend: Docker and `wslc` fill the same fields from `--format json`, Podman
//! adds pods and subnets, and unsupported fields stay empty.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerStatus {
    #[default]
    Unknown,
    Available,
    /// The CLI is missing or cannot reach any daemon.
    Unavailable {
        detail: Option<String>,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContainerSnapshot {
    pub status: ContainerStatus,
    #[serde(default)]
    pub containers: Vec<Container>,
    #[serde(default)]
    pub images: Vec<Image>,
    #[serde(default)]
    pub volumes: Vec<Volume>,
    #[serde(default)]
    pub networks: Vec<Network>,
    #[serde(default)]
    pub pods: Vec<Pod>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,
    /// Human status line (`Up 2 minutes`); empty when the engine omits it.
    pub status: String,
    pub ports: Option<String>,
    pub created: Option<String>,
    /// Host-side PID of the container's init process, not the PID inside the
    /// container namespace. Engines report it only while the container runs.
    #[serde(default)]
    pub pid: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Image {
    pub id: String,
    pub repository: String,
    pub tag: String,
    /// Engine-formatted size (`1.2GB` on Docker, bytes on Podman).
    pub size: String,
    /// Engine-formatted creation time; unix seconds on Podman.
    pub created: String,
    /// Parsed size for sorting and humanized display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    /// Parsed creation time (unix seconds) for sorting and humanized display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mount_point: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Network {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    /// Only Podman reports subnets in its list output.
    pub subnets: Vec<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pod {
    pub id: String,
    pub name: String,
    pub status: String,
    pub created: Option<String>,
    pub containers: usize,
}

impl Container {
    pub fn matches_query(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        [&self.id, &self.name, &self.image, &self.state, &self.status]
            .iter()
            .any(|value| value.to_lowercase().contains(&query))
    }
}

impl Pod {
    pub fn matches_query(&self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        [&self.id, &self.name, &self.status]
            .iter()
            .any(|value| value.to_lowercase().contains(&query))
    }
}

/// Row identity for the virtualized list: sampled status must not reset scroll.
pub fn container_row_signature(container: &Container) -> u64 {
    let mut hasher = DefaultHasher::new();
    container.id.hash(&mut hasher);
    hasher.finish()
}

pub fn pod_row_signature(pod: &Pod) -> u64 {
    let mut hasher = DefaultHasher::new();
    pod.id.hash(&mut hasher);
    hasher.finish()
}

/// Ordering the image table can apply; `None` keeps the engine's own order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageSort {
    Size,
    Created,
}

/// Sorts images by parsed metadata. Rows whose value is missing stay last in
/// both directions so an unparsed engine field cannot hide real images.
pub fn sort_images(images: &mut [Image], sort: ImageSort, descending: bool) {
    images.sort_by(|left, right| {
        let (left_value, right_value) = match sort {
            ImageSort::Size => (left.size_bytes, right.size_bytes),
            ImageSort::Created => (
                left.created_at.map(|value| value as u64),
                right.created_at.map(|value| value as u64),
            ),
        };
        match (left_value, right_value) {
            (Some(left), Some(right)) => {
                let order = left.cmp(&right);
                if descending { order.reverse() } else { order }
            }
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
}

pub fn container_state_label_key(state: &str) -> &'static str {
    match state.trim().to_ascii_lowercase().as_str() {
        "running" => "sidebar.host_container.states.running",
        "exited" => "sidebar.host_container.states.exited",
        "created" => "sidebar.host_container.states.created",
        "paused" => "sidebar.host_container.states.paused",
        "restarting" => "sidebar.host_container.states.restarting",
        "removing" => "sidebar.host_container.states.removing",
        "stopped" => "sidebar.host_container.states.stopped",
        "dead" => "sidebar.host_container.states.dead",
        _ => "sidebar.host_container.states.unknown",
    }
}

pub fn pod_state_label_key(status: &str) -> &'static str {
    match status.trim().to_ascii_lowercase().as_str() {
        "running" => "sidebar.host_container.states.running",
        "exited" => "sidebar.host_container.states.exited",
        "created" => "sidebar.host_container.states.created",
        "paused" => "sidebar.host_container.states.paused",
        "degraded" => "sidebar.host_container.states.degraded",
        "stopped" => "sidebar.host_container.states.stopped",
        _ => "sidebar.host_container.states.unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::{Image, ImageSort, sort_images};

    fn image(id: &str, size_bytes: Option<u64>, created_at: Option<i64>) -> Image {
        Image {
            id: id.to_string(),
            repository: format!("repo/{id}"),
            tag: "latest".to_string(),
            size: size_bytes
                .map(|bytes| bytes.to_string())
                .unwrap_or_default(),
            created: created_at.map(|time| time.to_string()).unwrap_or_default(),
            size_bytes,
            created_at,
        }
    }

    #[test]
    fn sort_images_orders_by_parsed_metadata_in_both_directions() {
        let mut images = vec![
            image("a", Some(300), Some(10)),
            image("b", Some(100), Some(30)),
            image("c", Some(200), Some(20)),
        ];
        sort_images(&mut images, ImageSort::Size, false);
        assert_eq!(
            images
                .iter()
                .map(|image| image.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "c", "a"]
        );
        sort_images(&mut images, ImageSort::Size, true);
        assert_eq!(
            images
                .iter()
                .map(|image| image.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "c", "b"]
        );
        sort_images(&mut images, ImageSort::Created, false);
        assert_eq!(
            images
                .iter()
                .map(|image| image.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "c", "b"]
        );
    }

    #[test]
    fn sort_images_keeps_unparsed_rows_last() {
        let mut images = vec![
            image("missing", None, None),
            image("old", Some(10), Some(1)),
            image("new", Some(20), Some(2)),
        ];
        sort_images(&mut images, ImageSort::Created, true);
        assert_eq!(
            images
                .iter()
                .map(|image| image.id.as_str())
                .collect::<Vec<_>>(),
            ["new", "old", "missing"]
        );
        sort_images(&mut images, ImageSort::Size, false);
        assert_eq!(
            images
                .iter()
                .map(|image| image.id.as_str())
                .collect::<Vec<_>>(),
            ["old", "new", "missing"]
        );
    }
}
