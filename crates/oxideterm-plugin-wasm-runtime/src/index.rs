// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// Sidecar runtime compatibility index downloaded by the host app.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmRuntimeIndex {
    pub schema_version: u32,
    #[serde(default)]
    pub generated_at: Option<String>,
    #[serde(default)]
    pub runtimes: Vec<WasmRuntimeDescriptor>,
}

/// One installable Wasm runtime build.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmRuntimeDescriptor {
    pub name: String,
    pub version: String,
    pub runtime_channel: String,
    pub supports: WasmRuntimeSupport,
    #[serde(default)]
    pub assets: Vec<WasmRuntimeAsset>,
}

/// Host-side constraints that a runtime build can serve.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmRuntimeSupport {
    #[serde(default)]
    pub oxideterm_channels: Vec<WasmRuntimeHostChannel>,
    #[serde(default)]
    pub oxideterm_versions: Vec<String>,
    #[serde(default)]
    pub plugin_protocol: Vec<u32>,
    #[serde(default)]
    pub wasm_guest_abi: Vec<u32>,
    #[serde(default)]
    pub wasi: Vec<String>,
}

/// Host app update channel as seen by the sidecar selector.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WasmRuntimeHostChannel {
    Stable,
    Beta,
    GpuiPreview,
}

/// Downloadable runtime artifact for a concrete target triple.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WasmRuntimeAsset {
    pub target: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub signature: Option<String>,
}

impl WasmRuntimeDescriptor {
    /// Returns whether this runtime can serve the concrete host/plugin ABI tuple.
    pub fn supports_host(
        &self,
        host_channel: WasmRuntimeHostChannel,
        host_version: &str,
        plugin_protocol: u32,
        wasm_guest_abi: u32,
        wasi: &str,
    ) -> bool {
        self.supports.oxideterm_channels.contains(&host_channel)
            && self
                .supports
                .oxideterm_versions
                .iter()
                .any(|requirement| version_matches_requirement(host_version, requirement))
            && self.supports.plugin_protocol.contains(&plugin_protocol)
            && self.supports.wasm_guest_abi.contains(&wasm_guest_abi)
            && self.supports.wasi.iter().any(|candidate| candidate == wasi)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ParsedVersion {
    core: Vec<u64>,
    prerelease: Option<Vec<PrereleasePart>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum PrereleasePart {
    Numeric(u64),
    Text(String),
}

fn version_matches_requirement(version: &str, requirement: &str) -> bool {
    let Some(version) = ParsedVersion::parse(version) else {
        return false;
    };

    requirement
        .split_whitespace()
        .all(|comparator| comparator_matches(&version, comparator))
}

fn comparator_matches(version: &ParsedVersion, comparator: &str) -> bool {
    let (operator, expected) = if let Some(value) = comparator.strip_prefix(">=") {
        (">=", value)
    } else if let Some(value) = comparator.strip_prefix("<=") {
        ("<=", value)
    } else if let Some(value) = comparator.strip_prefix('>') {
        (">", value)
    } else if let Some(value) = comparator.strip_prefix('<') {
        ("<", value)
    } else if let Some(value) = comparator.strip_prefix('=') {
        ("=", value)
    } else {
        ("=", comparator)
    };

    let Some(expected) = ParsedVersion::parse(expected) else {
        return false;
    };
    let ordering = version.cmp(&expected);

    match operator {
        ">=" => ordering != Ordering::Less,
        "<=" => ordering != Ordering::Greater,
        ">" => ordering == Ordering::Greater,
        "<" => ordering == Ordering::Less,
        "=" => ordering == Ordering::Equal,
        _ => false,
    }
}

impl ParsedVersion {
    fn parse(raw: &str) -> Option<Self> {
        let without_build = raw.trim().trim_start_matches('v').split('+').next()?;
        let mut parts = without_build.splitn(2, '-');
        let core = parts
            .next()?
            .split('.')
            .map(|part| part.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        if core.is_empty() {
            return None;
        }
        let prerelease = parts.next().map(|value| {
            value
                .split('.')
                .map(|part| {
                    part.parse::<u64>()
                        .map(PrereleasePart::Numeric)
                        .unwrap_or_else(|_| PrereleasePart::Text(part.to_string()))
                })
                .collect()
        });
        Some(Self { core, prerelease })
    }
}

impl Ord for ParsedVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        let core_len = self.core.len().max(other.core.len());
        for index in 0..core_len {
            let left = self.core.get(index).copied().unwrap_or_default();
            let right = other.core.get(index).copied().unwrap_or_default();
            match left.cmp(&right) {
                Ordering::Equal => {}
                ordering => return ordering,
            }
        }

        // Stable releases sort after prereleases with the same core version.
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(left), Some(right)) => compare_prerelease(left, right),
        }
    }
}

impl PartialOrd for ParsedVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease(left: &[PrereleasePart], right: &[PrereleasePart]) -> Ordering {
    for (left_part, right_part) in left.iter().zip(right.iter()) {
        let ordering = match (left_part, right_part) {
            (PrereleasePart::Numeric(left), PrereleasePart::Numeric(right)) => left.cmp(right),
            (PrereleasePart::Numeric(_), PrereleasePart::Text(_)) => Ordering::Less,
            (PrereleasePart::Text(_), PrereleasePart::Numeric(_)) => Ordering::Greater,
            (PrereleasePart::Text(left), PrereleasePart::Text(right)) => left.cmp(right),
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_descriptor() -> WasmRuntimeDescriptor {
        serde_json::from_value(serde_json::json!({
            "name": "oxideterm-wasm-runtime",
            "version": "0.1.0",
            "runtimeChannel": "stable",
            "supports": {
                "oxidetermChannels": ["stable", "beta", "gpui-preview"],
                "oxidetermVersions": [
                    ">=2.0.0 <3.0.0",
                    ">=2.0.0-beta.0 <3.0.0",
                    ">=2.0.0-gpui-preview.0 <3.0.0"
                ],
                "pluginProtocol": [1],
                "wasmGuestAbi": [1],
                "wasi": ["preview1"]
            },
            "assets": []
        }))
        .unwrap()
    }

    #[test]
    fn runtime_index_parses_host_update_channels() {
        let descriptor = sample_descriptor();

        assert!(
            descriptor
                .supports
                .oxideterm_channels
                .contains(&WasmRuntimeHostChannel::Stable)
        );
        assert!(
            descriptor
                .supports
                .oxideterm_channels
                .contains(&WasmRuntimeHostChannel::Beta)
        );
        assert!(
            descriptor
                .supports
                .oxideterm_channels
                .contains(&WasmRuntimeHostChannel::GpuiPreview)
        );
    }

    #[test]
    fn runtime_supports_stable_beta_and_gpui_preview_hosts() {
        let descriptor = sample_descriptor();

        assert!(descriptor.supports_host(
            WasmRuntimeHostChannel::Stable,
            "2.0.0",
            1,
            1,
            "preview1"
        ));
        assert!(descriptor.supports_host(
            WasmRuntimeHostChannel::Beta,
            "2.0.0-beta.4",
            1,
            1,
            "preview1"
        ));
        assert!(descriptor.supports_host(
            WasmRuntimeHostChannel::GpuiPreview,
            "2.0.0-gpui-preview.8",
            1,
            1,
            "preview1"
        ));
    }

    #[test]
    fn runtime_rejects_wrong_host_channel_or_abi() {
        let mut descriptor = sample_descriptor();
        descriptor.supports.oxideterm_channels = vec![WasmRuntimeHostChannel::Stable];

        assert!(!descriptor.supports_host(
            WasmRuntimeHostChannel::GpuiPreview,
            "2.0.0-gpui-preview.8",
            1,
            1,
            "preview1"
        ));
        assert!(!descriptor.supports_host(
            WasmRuntimeHostChannel::Stable,
            "2.0.0",
            2,
            1,
            "preview1"
        ));
        assert!(!descriptor.supports_host(
            WasmRuntimeHostChannel::Stable,
            "2.0.0",
            1,
            2,
            "preview1"
        ));
        assert!(!descriptor.supports_host(
            WasmRuntimeHostChannel::Stable,
            "2.0.0",
            1,
            1,
            "preview2"
        ));
    }

    #[test]
    fn stable_host_range_does_not_accidentally_cover_prerelease_hosts() {
        let mut descriptor = sample_descriptor();
        descriptor.supports.oxideterm_channels = vec![WasmRuntimeHostChannel::Stable];
        descriptor.supports.oxideterm_versions = vec![">=2.0.0 <3.0.0".to_string()];

        assert!(!descriptor.supports_host(
            WasmRuntimeHostChannel::Stable,
            "2.0.0-beta.1",
            1,
            1,
            "preview1"
        ));
    }
}
