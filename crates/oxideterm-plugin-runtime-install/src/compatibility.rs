// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Validation for the runtime compatibility index published with a release.

use serde::Deserialize;

const WASM_GUEST_ABI_VERSION: u32 = 1;
const WASI_PROFILE: &str = "preview1";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WasmRuntimeReleaseIndex {
    runtimes: Vec<WasmRuntimeReleaseDescriptor>,
}

#[derive(Debug, Deserialize)]
struct WasmRuntimeReleaseDescriptor {
    supports: WasmRuntimeReleaseSupport,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WasmRuntimeReleaseSupport {
    oxideterm_channels: Vec<String>,
    oxideterm_versions: Vec<String>,
    plugin_protocol: Vec<u32>,
    wasm_guest_abi: Vec<u32>,
    wasi: Vec<String>,
}

pub(super) fn validate_runtime_index(index: &str, host_version: &str) -> Result<(), String> {
    let index: WasmRuntimeReleaseIndex = serde_json::from_str(index)
        .map_err(|error| format!("Failed to parse Wasm runtime index: {error}"))?;
    let host_channel = current_oxideterm_runtime_channel(host_version);
    let compatible = index.runtimes.iter().any(|runtime| {
        runtime
            .supports
            .oxideterm_channels
            .iter()
            .any(|channel| channel == host_channel)
            && runtime
                .supports
                .oxideterm_versions
                .iter()
                .any(|requirement| {
                    runtime_requirement_mentions_host_channel(
                        requirement,
                        host_channel,
                        host_version,
                    )
                })
            && runtime
                .supports
                .plugin_protocol
                .contains(&oxideterm_plugin_protocol::NATIVE_PLUGIN_PROTOCOL_VERSION)
            && runtime
                .supports
                .wasm_guest_abi
                .contains(&WASM_GUEST_ABI_VERSION)
            && runtime
                .supports
                .wasi
                .iter()
                .any(|profile| profile == WASI_PROFILE)
    });
    compatible.then_some(()).ok_or_else(|| {
        format!(
            "Latest Wasm runtime does not declare support for OxideTerm {host_version} ({host_channel})"
        )
    })
}

fn runtime_requirement_mentions_host_channel(
    requirement: &str,
    host_channel: &str,
    host_version: &str,
) -> bool {
    match host_channel {
        "beta" => requirement.contains("beta"),
        _ => !host_version.contains('-') && !requirement.contains('-'),
    }
}

fn current_oxideterm_runtime_channel(host_version: &str) -> &'static str {
    let version = host_version.to_ascii_lowercase();
    if version.contains('-') {
        "beta"
    } else {
        "stable"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMPATIBLE_INDEX: &str = r#"{
        "runtimes": [{
            "supports": {
                "oxidetermChannels": ["beta"],
                "oxidetermVersions": [">=2.0.0-beta.0 <3.0.0"],
                "pluginProtocol": [1],
                "wasmGuestAbi": [1],
                "wasi": ["preview1"]
            }
        }]
    }"#;

    #[test]
    fn index_requires_the_current_channel_and_protocol_contract() {
        validate_runtime_index(COMPATIBLE_INDEX, "2.0.0-beta.15").unwrap();

        let incompatible =
            COMPATIBLE_INDEX.replace("\"pluginProtocol\": [1]", "\"pluginProtocol\": [2]");
        let error = validate_runtime_index(&incompatible, "2.0.0-beta.15").unwrap_err();
        assert!(error.contains("does not declare support"));
    }

    #[test]
    fn stable_hosts_reject_prerelease_only_requirements() {
        let error = validate_runtime_index(COMPATIBLE_INDEX, "2.0.0").unwrap_err();

        assert!(error.contains("stable"));
    }
}
