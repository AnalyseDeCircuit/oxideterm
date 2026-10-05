// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Canonical permission declarations and persisted approval comparisons.

use std::collections::HashSet;

use sha2::{Digest, Sha256};

use crate::{
    NativePluginConfigEntry, NativePluginManifest, NativePluginRuntimePlan,
    native_runtime_kind_label,
};

/// Process plugins execute outside the WASM sandbox and therefore require an
/// explicit trust decision before the host starts them.
pub const NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY: &str = "runtime.process.trusted";

/// Normalizes declared capability names without treating an empty declaration as
/// an empty plugin data plane.
pub fn normalize_native_plugin_capabilities(
    capabilities: &[String],
) -> Result<Vec<String>, String> {
    let mut normalized = Vec::with_capacity(capabilities.len());
    let mut seen = HashSet::with_capacity(capabilities.len());

    for capability in capabilities {
        let capability = capability.trim();
        if capability.is_empty() {
            return Err("Plugin permission capabilities cannot be empty".to_string());
        }
        if capability.contains('*') {
            return Err(format!(
                "Plugin permission capability \"{capability}\" cannot contain wildcards"
            ));
        }
        if !seen.insert(capability.to_string()) {
            return Err(format!(
                "Plugin permission capability \"{capability}\" is declared more than once"
            ));
        }
        normalized.push(capability.to_string());
    }

    // Ordering must not cause a permission prompt or invalidate an approval.
    normalized.sort_unstable();
    Ok(normalized)
}

/// Returns a stable SHA-256 fingerprint for a normalized capability set.
pub fn native_plugin_capabilities_fingerprint(capabilities: &[String]) -> Result<String, String> {
    let normalized = normalize_native_plugin_capabilities(capabilities)?;
    let mut digest = Sha256::new();
    for capability in normalized {
        // Length prefixes avoid ambiguity if future capability names gain separators.
        digest.update((capability.len() as u64).to_be_bytes());
        digest.update(capability.as_bytes());
    }
    Ok(format!("sha256:{:x}", digest.finalize()))
}

/// Returns the complete sensitive capability request for a manifest and its
/// selected runtime boundary.
pub fn native_plugin_requested_capabilities(
    manifest: &NativePluginManifest,
    runtime_plan: &NativePluginRuntimePlan,
) -> Result<Vec<String>, String> {
    normalized_requested_capabilities(
        &manifest.permissions.capabilities,
        native_runtime_kind_label(runtime_plan),
    )
}

fn normalized_requested_capabilities(
    declared_capabilities: &[String],
    runtime_kind: &str,
) -> Result<Vec<String>, String> {
    let mut capabilities = normalize_native_plugin_capabilities(declared_capabilities)?;
    if matches!(runtime_kind, "process" | "acp" | "remote-desktop")
        && capabilities
            .binary_search_by(|candidate| {
                candidate
                    .as_str()
                    .cmp(NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY)
            })
            .is_err()
    {
        capabilities.push(NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY.to_string());
        capabilities.sort_unstable();
    }
    Ok(capabilities)
}

/// Checks whether the current runtime still requests only previously approved
/// sensitive capabilities.
pub fn native_plugin_capability_approval_matches(
    manifest: &NativePluginManifest,
    runtime_kind: &str,
    config: &NativePluginConfigEntry,
) -> bool {
    // Version is audit metadata. A version-only update must not create another
    // prompt when the runtime boundary and requested capabilities stay safe.
    if config.approved_runtime_kind.as_deref() != Some(runtime_kind) {
        return false;
    }

    let Ok(requested) =
        normalized_requested_capabilities(&manifest.permissions.capabilities, runtime_kind)
    else {
        return false;
    };
    let Ok(approved) = normalize_native_plugin_capabilities(&config.approved_capabilities) else {
        return false;
    };
    // Removing a capability narrows access and therefore does not need renewed
    // consent; any newly requested capability invalidates the old approval.
    requested
        .iter()
        .all(|capability| approved.binary_search(capability).is_ok())
}

/// Reports whether activation is waiting for the user to approve sensitive
/// reads, side effects, or an unsandboxed process runtime.
pub fn native_plugin_requires_permission_review(
    manifest: &NativePluginManifest,
    runtime_plan: &NativePluginRuntimePlan,
    config: &NativePluginConfigEntry,
) -> bool {
    let Ok(requested) = native_plugin_requested_capabilities(manifest, runtime_plan) else {
        return true;
    };
    if requested.is_empty() {
        return false;
    }
    !native_plugin_capability_approval_matches(
        manifest,
        native_runtime_kind_label(runtime_plan),
        config,
    )
}
